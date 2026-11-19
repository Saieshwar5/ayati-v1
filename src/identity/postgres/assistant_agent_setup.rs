//! A member's assistant helps them set up an agent (schema 139).
//!
//! In New agent and on an agent's Instructions tab the member says what the
//! agent should do, and their own assistant drafts its ground rules and skills.
//!
//! - **Asked like a draft.** Each turn is the member's message in the "Agent
//!   setup" thread of their assistant's room. The setup so far travels with the
//!   task, not in the message, so the room reads as a conversation.
//! - **Handed back in parts.** The assistant calls `write_agent_setup` once per
//!   part. Parts are kept on the turn; nothing reaches an agent.
//! - **Checked like the writer's.** Reading a turn runs the skill writer's
//!   checks (skill_writer/answer.rs). The member reviews the drafts and saves
//!   them through the ordinary agent and skill routes.
//! - **Rules stay a person's.** For someone's assistant no ground rules are
//!   drafted: only its person writes them.
//! - **Or asked for in chat.** In its own chat with its person the assistant
//!   may begin a draft for a new agent itself (assistant_agent_setup_chat.rs).
use super::super::*;
use crate::identity::chat::model::{Job, NewMessage};
use crate::identity::skill_writer::{Request as Setup, answer, guide};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashSet;

#[path = "assistant_agent_setup_chat.rs"]
pub(in crate::identity::postgres) mod chat;
#[cfg(test)]
#[path = "assistant_agent_setup_chat_tests.rs"]
mod chat_tests;
#[cfg(test)]
#[path = "assistant_agent_setup_tests.rs"]
mod tests;

/// The most parts one turn keeps.
const MOST_PARTS: i32 = 40;
/// The most bytes of skill steps the assistant is shown in one turn.
const STEPS_SHOWN: usize = 24_000;
const NOT_A_SETUP: &str = "Use this only for your own person: when they ask you in your chat with them for a new agent, or for help in New agent or on an agent's page.";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SetupRequest {
    /// The message's id: the same id asks once.
    pub id: String,
    pub setup: Setup,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Part {
    part: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    text: Option<String>,
}

pub(in crate::identity::postgres) async fn ready(db: &impl ConnectionTrait) -> Result<bool> {
    Ok(query(
        db,
        "SELECT to_regclass('relay_agent_setups') IS NOT NULL AS ready",
        vec![],
    )
    .await?
    .context("Missing schema check")?
    .try_get("", "ready")?)
}

/// One part as it is kept: trimmed, cut to size, with only its own fields.
fn kept(input: &Part) -> std::result::Result<Value, &'static str> {
    let field = |value: &Option<String>, limit: usize| {
        value
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(|v| v.chars().take(limit).collect::<String>())
    };
    let name = field(&input.name, 120);
    let text = field(&input.text, 6000);
    match input.part.as_str() {
        "ground_rules" => text
            .map(|text| json!({"part": "ground_rules", "text": text}))
            .ok_or("Send the ground rules as text."),
        "skill" => match (name, field(&input.description, 1024), text) {
            (Some(name), Some(description), Some(text)) => {
                Ok(json!({"part": "skill", "name": name, "description": description, "text": text}))
            }
            _ => Err("A skill needs a name, a description and its steps as text."),
        },
        "remove_skill" | "reuse_skill" | "needs_app" => name
            .map(|name| json!({"part": input.part, "name": name}))
            .ok_or("Send its name."),
        "name" | "job_title" | "used" => text
            .map(|text| json!({"part": input.part, "text": text}))
            .ok_or("Send it as text."),
        _ => Err("Unknown part."),
    }
}

/// What a turn's parts add up to, before the checks.
#[derive(Default)]
struct Drafted {
    raw: answer::Raw,
    /// The agent's name, when its assistant chose one in chat.
    name: Option<String>,
    job_title: Option<String>,
    used: Vec<String>,
    notes: Vec<String>,
}

/// Folds the parts in the order they were handed back: a later ground rules or
/// skill of the same name replaces the earlier one.
fn drafted(parts: &[Value], reply: String, rules: bool) -> Drafted {
    let mut out = Drafted::default();
    out.raw.reply = reply;
    let text = |part: &Value, key: &str| part[key].as_str().unwrap_or_default().to_owned();
    for part in parts {
        match part["part"].as_str().unwrap_or_default() {
            "ground_rules" if rules => out.raw.ground_rules = Some(text(part, "text")),
            "ground_rules" => {
                let note =
                    "Ground rules were left out: only its person writes an assistant's rules.";
                if !out.notes.iter().any(|n| n == note) {
                    out.notes.push(note.into());
                }
            }
            "skill" => {
                let name = answer::slug(&text(part, "name"));
                out.raw.skills.retain(|s| answer::slug(&s.name) != name);
                out.raw.skills.push(answer::RawSkill {
                    name,
                    description: text(part, "description"),
                    steps: text(part, "text"),
                });
            }
            "remove_skill" => {
                // It also drops a skill drafted earlier in these parts.
                let name = answer::slug(&text(part, "name"));
                out.raw.skills.retain(|s| answer::slug(&s.name) != name);
                out.raw.remove.push(name);
            }
            "reuse_skill" => out.raw.reuse.push(text(part, "name")),
            "needs_app" => out.raw.missing_apps.push(text(part, "name")),
            "name" => {
                out.name =
                    Some(answer::one_line(&text(part, "text"), 80)).filter(|name| !name.is_empty());
            }
            "job_title" => {
                out.job_title = Some(answer::one_line(&text(part, "text"), 80))
                    .filter(|title| !title.is_empty());
            }
            "used" => {
                let line = answer::one_line(&text(part, "text"), 200);
                if !line.is_empty() && out.used.len() < 6 && !out.used.contains(&line) {
                    out.used.push(line);
                }
            }
            _ => {}
        }
    }
    out
}

impl PgStore {
    /// Whether the member's assistant can help set up an agent here, and who it is.
    pub async fn agent_setup_status(&self, user: &User, w: &str) -> Result<Value> {
        role(&self.db, w, &user.id).await?;
        // Its tools reach it only on pooled executors.
        if !self.pooled || !ready(&self.db).await? || !super::ready(&self.db).await? {
            return Ok(json!({"available": false}));
        }
        let found = json_rows(
            &self.db,
            "SELECT jsonb_build_object('id',a.id,'name',a.name,'avatar',a.settings->>'avatar') AS data
             FROM relay_assistants s JOIN relay_shared_agents a ON a.workspace=s.workspace AND a.id=s.agent
             WHERE s.workspace=$1 AND s.user_id=$2 AND COALESCE((a.settings->>'paused')::boolean,false)=false",
            vec![w.into(), user.id.clone().into()],
        )
        .await?;
        Ok(match found.into_iter().next() {
            // People can read what it is told, as they can the writer's guide.
            Some(assistant) => json!({"available": true, "assistant": assistant,
                "guide": guide::ASSISTANT_GUIDE.trim()}),
            None => json!({"available": false}),
        })
    }

    /// One turn of the setup chat: asks the member's assistant in its own room.
    /// Nothing is drafted or saved here; the same id asks once.
    pub(crate) async fn agent_setup_request(
        &self,
        user: &User,
        w: &str,
        input: &SetupRequest,
    ) -> Result<(String, Job)> {
        let setup = &input.setup;
        setup.validate()?;
        let (said, earlier) = setup
            .messages
            .split_last()
            .context("Say what the agent should do")?;
        ensure!(
            said.text.trim().chars().count() <= 2000,
            "Say it in fewer words: up to 2,000 characters"
        );
        ensure!(
            setup.skills.iter().map(|s| s.steps.len()).sum::<usize>() <= STEPS_SHOWN,
            "These skills are too long for your assistant to read at once. Shorten or remove one, or edit them by hand."
        );
        let assistant = self
            .assistant_of(user, w)
            .await?
            .context("You have no assistant yet")?;
        // Only its person writes an assistant's rules, so none are drafted for one.
        let mut rules = true;
        if let Some(agent) = &setup.agent {
            self.managed_agent(user, w, agent, true).await?;
            rules = !super::is_assistant(&self.db, w, agent).await?;
        }
        let context = self
            .skill_writer_context(user, w, setup.agent.as_deref(), &setup.connections)
            .await?;
        let mut shown = guide::setup(setup, &context);
        shown["draft_ground_rules"] = json!(rules);
        // The last turns of this setup, so each task stands on its own.
        let from = earlier.len().saturating_sub(8);
        shown["conversation"] = earlier[from..]
            .iter()
            .map(|m| {
                let who = if m.role == "agent" {
                    "you"
                } else {
                    "your person"
                };
                json!({"from": who, "text": m.text.chars().take(600).collect::<String>()})
            })
            .collect();
        let name = setup.name.trim();
        let request = json!({
            "name": name,
            "agent": setup.agent,
            "own": setup.skills.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
            "rules": rules,
            "shown": shown,
        });
        // A mention would ask another agent; the member's words ask only their assistant.
        let words = said.text.trim().replace("](mention:", "](");
        let content = format!(
            "Setting up {}: {words}",
            if name.is_empty() { "a new agent" } else { name }
        );
        let tx = self.workspace_tx(w).await?;
        ensure!(ready(&tx).await?, "Your assistant can't help with this yet");
        let (room, thread) =
            super::run_place(&tx, w, &assistant, &user.id, "agent-setup", "Agent setup")
                .await?
                .context("Your chat with your assistant is missing")?;
        let message = NewMessage {
            id: input.id.clone(),
            content,
            agent: Some(assistant),
            agents: vec![],
            thread: Some(thread),
            reply_to: None,
            attachments: vec![],
        };
        let job = self
            .as_owner_in(&tx, user, w, &room, &message)
            .await?
            .into_iter()
            .next();
        // A retried request finds the reply it already asked for.
        let job: Job = match job {
            Some(job) => job,
            None => {
                let payload: Value = query(
                    &tx,
                    "SELECT payload FROM relay_chat_jobs WHERE workspace=$1 AND request_message=$2",
                    vec![w.into(), input.id.clone().into()],
                )
                .await?
                .context("Your assistant's answer is unavailable")?
                .try_get("", "payload")?;
                serde_json::from_value(payload)?
            }
        };
        execute(
            &tx,
            "INSERT INTO relay_agent_setups(workspace,reply,user_id,request) VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING",
            vec![w.into(), job.reply.clone().into(), user.id.clone().into(), request.into()],
        )
        .await?;
        tx.commit().await?;
        Ok((room, job))
    }

    /// For an assistant's request: the setup it drafts from, when this task is
    /// a turn of the setup chat.
    pub(crate) async fn agent_setup_context(&self, job: &Job) -> Result<Option<String>> {
        if !ready(&self.db).await? {
            return Ok(None);
        }
        let row = query(
            &self.db,
            "SELECT request->'shown' AS shown FROM relay_agent_setups WHERE workspace=$1 AND reply=$2 AND user_id=$3",
            vec![job.workspace.clone().into(), job.reply.clone().into(), job.requester.clone().into()],
        )
        .await?;
        Ok(match row {
            Some(row) => match row.try_get::<Value>("", "shown")? {
                // A draft it began itself in chat has no setup to show.
                Value::Null => None,
                shown => Some(guide::assistant_setup(&shown)),
            },
            // An ordinary task in its own chat: the draft it made there earlier.
            None => chat::carried(&self.db, job).await?,
        })
    }

    /// `write_agent_setup`: keep one drafted part on this turn. Nothing reaches
    /// an agent; the member sees the parts when the assistant has answered.
    pub(crate) async fn write_agent_setup(&self, job: &Job, input: &Part) -> Result<Value> {
        let invalid = |message: &str| json!({"state": "invalid", "message": message});
        let tx = self.workspace_tx(&job.workspace).await?;
        super::super::control_actions::authorize(&tx, job, true).await?;
        if !ready(&tx).await? {
            return Ok(invalid(NOT_A_SETUP));
        }
        let part = match kept(input) {
            Ok(part) => part,
            Err(message) => return Ok(invalid(message)),
        };
        // Only the turn's own assistant, working for the member who asked.
        let turn = || {
            query(
                &tx,
                "SELECT jsonb_array_length(s.parts) AS parts FROM relay_agent_setups s
                 JOIN relay_assistants a ON a.workspace=s.workspace AND a.user_id=s.user_id AND a.agent=$3
                 WHERE s.workspace=$1 AND s.reply=$2 AND s.user_id=$4 FOR UPDATE OF s",
                vec![
                    job.workspace.clone().into(),
                    job.reply.clone().into(),
                    job.agent.clone().into(),
                    job.requester.clone().into(),
                ],
            )
        };
        let row = match turn().await? {
            Some(row) => Some(row),
            // In its own chat with its person it may begin a draft itself.
            None if chat::begin(&tx, job).await? => turn().await?,
            None => None,
        };
        let Some(row) = row else {
            return Ok(invalid(NOT_A_SETUP));
        };
        if row.try_get::<i32>("", "parts")? >= MOST_PARTS {
            return Ok(
                json!({"state": "full", "message": "That is the most parts for one answer. Answer your person now."}),
            );
        }
        execute(
            &tx,
            "UPDATE relay_agent_setups SET parts=parts||jsonb_build_array($3::jsonb) WHERE workspace=$1 AND reply=$2",
            vec![job.workspace.clone().into(), job.reply.clone().into(), part.into()],
        )
        .await?;
        tx.commit().await?;
        Ok(
            json!({"state": "noted", "message": "Noted. Nothing is saved: your person sees it when you answer."}),
        )
    }

    /// One turn's drafts for the member who asked, checked like uploaded
    /// skills. While the assistant works only its state is returned.
    pub async fn agent_setup(&self, user: &User, w: &str, reply: &str) -> Result<Value> {
        ensure!(ready(&self.db).await?, "Setup not found");
        let row = query(
            &self.db,
            "SELECT s.request,s.parts,m.state,m.content FROM relay_agent_setups s
             JOIN relay_chat_messages m ON m.workspace=s.workspace AND m.id=s.reply
             WHERE s.workspace=$1 AND s.reply=$2 AND s.user_id=$3",
            vec![w.into(), reply.into(), user.id.clone().into()],
        )
        .await?
        .context("Setup not found")?;
        let state: String = row.try_get("", "state")?;
        if state != "complete" {
            return Ok(json!({"state": state}));
        }
        let request: Value = row.try_get("", "request")?;
        let parts: Value = row.try_get("", "parts")?;
        let context = self
            .skill_writer_context(user, w, request["agent"].as_str(), &[])
            .await?;
        let own: HashSet<&str> = request["own"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        let Drafted {
            raw,
            name,
            job_title,
            used,
            notes,
        } = drafted(
            parts.as_array().map(Vec::as_slice).unwrap_or_default(),
            row.try_get("", "content")?,
            request["rules"] != false,
        );
        // A draft begun in chat is for a new agent the assistant named.
        let in_chat = request["from"] == "chat";
        let agent_name = match (&name, in_chat) {
            (Some(name), true) => name.as_str(),
            _ => request["name"].as_str().unwrap_or_default(),
        };
        let mut answer = answer::checked(raw, agent_name, &own, &context);
        answer.notes.extend(notes);
        let mut out = serde_json::to_value(answer)?;
        out["state"] = json!("complete");
        out["job_title"] = json!(job_title);
        out["used"] = json!(used);
        if in_chat {
            let agent = request["agent_id"].as_str().unwrap_or_default();
            out["from"] = json!("chat");
            out["name"] = json!(name);
            out["agent_id"] = json!(agent);
            out["created"] = json!(!self.agent_is_new(w, agent).await?);
            // A later reply carried this draft on: Chat shows that one.
            out["replaced"] = json!(
                query(
                    &self.db,
                    "SELECT 1 AS found FROM relay_agent_setups s JOIN relay_agent_setups n ON n.workspace=s.workspace AND n.user_id=s.user_id
                       AND n.request->>'agent_id'=s.request->>'agent_id' AND jsonb_array_length(n.parts)>0 AND (n.created_ms,n.reply)>(s.created_ms,s.reply)
                     WHERE s.workspace=$1 AND s.reply=$2 LIMIT 1",
                    vec![w.into(), reply.into()],
                )
                .await?
                .is_some()
            );
            // Each skill's own id, the same on every read, so creating twice saves it once.
            for skill in out["skills"].as_array_mut().into_iter().flatten() {
                let seed = format!("{reply}\n{}", skill["name"].as_str().unwrap_or_default());
                let hash = ring::digest::digest(&ring::digest::SHA256, seed.as_bytes());
                let mut bytes = [0u8; 16];
                bytes.copy_from_slice(&hash.as_ref()[..16]);
                skill["id"] = json!(
                    uuid::Builder::from_random_bytes(bytes)
                        .into_uuid()
                        .to_string()
                );
            }
        }
        Ok(out)
    }
}

pub(crate) use migration::AgentSetups;
mod migration {
    use sea_orm_migration::prelude::*;
    pub(crate) struct AgentSetups;
    impl MigrationName for AgentSetups {
        fn name(&self) -> &str {
            "m20261119_000139_agent_setups"
        }
    }
    #[async_trait::async_trait]
    impl MigrationTrait for AgentSetups {
        async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
            m.get_connection()
                .execute_unprepared(include_str!("assistant_agent_setup.sql"))
                .await?;
            Ok(())
        }
        async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
            Err(DbErr::Custom(
                "Agent setups require a forward migration".into(),
            ))
        }
    }
}

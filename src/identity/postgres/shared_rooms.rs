//! Shared rooms: conversations more than one person reads, such as channels and
//! group chats. Everyone there reads the agent's reply, so a shared room has one
//! working brief they all see, and an agent never answers there from the
//! asker's private information. It moves the request to the asker's direct
//! conversation with the agent instead (`continue_privately`, schema 55).
use super::*;
use crate::identity::chat::model::{Job, NewMessage};
use serde_json::json;
#[path = "shared_rooms_migration.rs"]
pub(super) mod migration;
#[cfg(test)]
#[path = "shared_rooms_tests.rs"]
mod tests;

/// Who reads a shared room's replies.
#[derive(Debug, PartialEq)]
pub(crate) struct Audience {
    /// Anyone in the workspace can open it.
    pub public: bool,
    pub people: i64,
}

/// `None` for a conversation only one person reads: a direct message, a
/// person's own group with agents, a Library question or an email thread.
pub(super) async fn audience(
    db: &impl ConnectionTrait,
    w: &str,
    chat: &str,
) -> Result<Option<Audience>> {
    let row = query(db, "SELECT c.kind,c.visibility='public' AS public,
        (SELECT count(*) FROM relay_chat_people p WHERE p.workspace=c.workspace AND p.chat=c.id) AS people
        FROM relay_chats c WHERE c.workspace=$1 AND c.id=$2", vec![w.into(), chat.into()])
        .await?
        .context("Conversation missing")?;
    let (kind, public, people): (String, bool, i64) = (
        row.try_get("", "kind")?,
        row.try_get("", "public")?,
        row.try_get("", "people")?,
    );
    Ok((kind != "direct" && (public || people > 1)).then_some(Audience { public, people }))
}

/// Whose working brief a task reads and writes: the room's, in a shared room;
/// otherwise the requester's own.
pub(super) async fn brief_owner(db: &impl ConnectionTrait, job: &Job) -> Result<String> {
    Ok(
        if audience(db, &job.workspace, &job.chat).await?.is_some() {
            String::new()
        } else {
            job.requester.clone()
        },
    )
}

/// The person who asked, when this task is a question moved from a shared room.
pub(super) async fn moved_from(db: &impl ConnectionTrait, job: &Job) -> Result<Option<Value>> {
    let ready = query(
        db,
        "SELECT to_regclass('relay_private_continuations') IS NOT NULL AS ready",
        vec![],
    )
    .await?
    .context("Missing schema check")?
    .try_get::<bool>("", "ready")?;
    if !ready {
        return Ok(None);
    }
    Ok(json_rows(db, "SELECT jsonb_build_object('id',m.author,'kind','user','display_name',u.display_name) AS data
        FROM relay_private_continuations p JOIN relay_chat_messages m ON m.workspace=p.workspace AND m.id=p.source_message
        LEFT JOIN relay_users u ON u.id=m.author WHERE p.workspace=$1 AND p.reply=$2",
        vec![job.workspace.clone().into(), job.reply.clone().into()]).await?.into_iter().next())
}

/// Mentions travel as Markdown links; outside their room they become plain names.
fn plain_mentions(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("](mention:") {
        let Some(open) = rest[..start].rfind('[') else {
            break;
        };
        let Some(close) = rest[start..].find(')') else {
            break;
        };
        out.push_str(&rest[..open]);
        out.push_str(&rest[open + 1..start]);
        rest = &rest[start + close + 1..];
    }
    out.push_str(rest);
    out
}

/// The person's direct conversation with the agent, opened if it doesn't exist.
async fn direct_room(
    tx: &DatabaseTransaction,
    w: &str,
    person: &str,
    agent: &str,
) -> Result<String> {
    let mut participants = vec![format!("user:{person}"), format!("agent:{agent}")];
    participants.sort();
    let key = serde_json::to_string(&participants)?;
    if let Some(row) = query(
        tx,
        "SELECT id FROM relay_chats WHERE workspace=$1 AND direct_key=$2",
        vec![w.into(), key.clone().into()],
    )
    .await?
    {
        return Ok(row.try_get("", "id")?);
    }
    let id = uuid::Uuid::new_v4().to_string();
    execute(tx, "INSERT INTO relay_chats(id,workspace,name,creator,kind,description,direct_key)
        SELECT $1,$2,a.name,$3,'direct','',$4 FROM relay_shared_agents a WHERE a.workspace=$2 AND a.id=$5",
        vec![id.clone().into(), w.into(), person.into(), key.into(), agent.into()]).await?;
    execute(
        tx,
        "INSERT INTO relay_chat_people(workspace,chat,user_id) VALUES($1,$2,$3)",
        vec![w.into(), id.clone().into(), person.into()],
    )
    .await?;
    execute(
        tx,
        "INSERT INTO relay_chat_agents(workspace,chat,agent,joined_sequence) VALUES($1,$2,$3,0)",
        vec![w.into(), id.clone().into(), agent.into()],
    )
    .await?;
    Ok(id)
}

const MOVED: &str = "Cuckoo started a private request with the same question in this person's direct conversation with you. Reply here with one short line saying you have answered privately. Do not put the private details here.";

impl PgStore {
    pub(crate) async fn room_audience(&self, job: &Job) -> Result<Option<Audience>> {
        let tx = self.read_tx(&job.workspace).await?;
        let found = audience(&tx, &job.workspace, &job.chat).await?;
        tx.commit().await?;
        Ok(found)
    }

    /// Moves a person's question from a shared room to their direct
    /// conversation with the agent, where it is answered with private context.
    /// Exact retries return the same private request.
    pub(crate) async fn continue_privately(&self, job: &Job) -> Result<Value> {
        let w = &job.workspace;
        let tx = self.workspace_tx(w).await?;
        super::control_actions::authorize(&tx, job, true).await?;
        if let Some(done) = json_rows(&tx, "SELECT jsonb_build_object('state','moved','conversation',chat,'reply',reply) AS data FROM relay_private_continuations WHERE workspace=$1 AND source=$2",
            vec![w.into(), job.reply.clone().into()]).await?.into_iter().next() {
            tx.commit().await?;
            return Ok(json!({"state":"moved","conversation":done["conversation"],"reply":done["reply"],"message":MOVED}));
        }
        let refuse = |state: &str, message: &str| Ok(json!({"state":state,"message":message}));
        if audience(&tx, w, &job.chat).await?.is_none() {
            return refuse(
                "invalid",
                "This conversation is already private: answer here.",
            );
        }
        let request = query(
            &tx,
            "SELECT m.id,m.author,m.kind,m.content,c.name FROM relay_chat_jobs j
            JOIN relay_chat_messages m ON m.workspace=j.workspace AND m.id=j.request_message
            JOIN relay_chats c ON c.workspace=m.workspace AND c.id=m.chat
            WHERE j.workspace=$1 AND j.reply=$2 AND j.work_class='interactive'",
            vec![w.into(), job.reply.clone().into()],
        )
        .await?;
        let Some(request) = request.filter(|r| {
            r.try_get::<String>("", "kind").is_ok_and(|k| k == "user")
                && r.try_get::<String>("", "author")
                    .is_ok_and(|a| a == job.requester)
        }) else {
            return refuse(
                "invalid",
                "Only a person's own question can move to a private conversation.",
            );
        };
        if !super::chat::agent_access(&tx, &job.requester, w, &job.agent).await? {
            return refuse(
                "denied",
                "The person who asked can't message you directly, so the answer can't move to a private conversation. Say so here, without the private details.",
            );
        }
        let room: String = request.try_get("", "name")?;
        let mut content = format!(
            "You asked in {room}: {}",
            plain_mentions(&request.try_get::<String>("", "content")?)
        );
        while content.len() > 12000 {
            content.pop();
        }
        let dm = direct_room(&tx, w, &job.requester, &job.agent).await?;
        let message = NewMessage {
            reply_to: None,
            attachments: vec![],
            thread: None,
            id: uuid::Uuid::new_v4().to_string(),
            content,
            agent: Some(job.agent.clone()),
        };
        let person = User {
            id: job.requester.clone(),
            email: String::new(),
            display_name: None,
        };
        let moved = self
            .private_in(&tx, &person, w, &dm, &message, &job.agent)
            .await?
            .context("The private request was not queued")?;
        execute(&tx, "INSERT INTO relay_private_continuations(workspace,source,source_message,chat,message,reply,created) VALUES($1,$2,$3,$4,$5,$6,$7)",
            vec![w.into(), job.reply.clone().into(), request.try_get::<String>("", "id")?.into(), dm.clone().into(), message.id.into(), moved.reply.clone().into(), now().into()]).await?;
        tx.commit().await?;
        Ok(json!({"state":"moved","conversation":dm,"reply":moved.reply,"message":MOVED}))
    }
}

//! What an assistant does in a thread (schema 128): suggest one, answer in a
//! running one, finish it; and the loop that hands it each new answer from
//! the other side. Every send is the member's own message (see the parent).
use super::*;
use crate::identity::chat::model::{AgentsPaused, BackgroundQueueFull, QueueFull};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Suggest {
    to: String,
    title: String,
    opening: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Reply {
    thread: String,
    key: String,
    text: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Finish {
    thread: String,
    summary: String,
}

fn invalid(message: impl Into<String>) -> Value {
    json!({"state":"invalid","message":message.into()})
}

/// The job is an assistant working for its own person.
async fn for_owner(tx: &DatabaseTransaction, job: &Job) -> Result<bool> {
    Ok(ready(tx).await?
        && query(
            tx,
            "SELECT 1 AS found FROM relay_assistants WHERE workspace=$1 AND agent=$2 AND user_id=$3",
            vec![job.workspace.clone().into(), job.agent.clone().into(), job.requester.clone().into()],
        )
        .await?
        .is_some())
}

/// The agent or person `to` names, by ID or exact name: (kind, id).
async fn named(tx: &DatabaseTransaction, job: &Job, to: &str) -> Result<Vec<(String, String)>> {
    let found = rows(
        tx,
        "SELECT 'agent' AS kind,a.id FROM relay_shared_agents a
          WHERE a.workspace=$1 AND (a.id=$2 OR lower(a.name)=lower($2))
            AND NOT EXISTS(SELECT 1 FROM relay_assistants s WHERE s.workspace=a.workspace AND s.agent=a.id)
         UNION ALL
         SELECT 'user',u.id FROM relay_members m JOIN relay_users u ON u.id=m.user_id
          WHERE m.workspace=$1 AND u.id<>$3 AND (u.id=$2 OR lower(btrim(u.display_name))=lower($2))",
        vec![job.workspace.clone().into(), to.trim().into(), job.requester.clone().into()],
    )
    .await?;
    found
        .into_iter()
        .map(|row| Ok((row.try_get("", "kind")?, row.try_get("", "id")?)))
        .collect()
}

impl PgStore {
    /// `suggest_thread`: propose a thread to the member. Nothing is sent until they start it.
    pub(crate) async fn suggest_thread(&self, job: &Job, input: &Suggest) -> Result<Value> {
        let tx = self.workspace_tx(&job.workspace).await?;
        super::super::super::control_actions::authorize(&tx, job, true).await?;
        if !for_owner(&tx, job).await? {
            return Ok(invalid(
                "Only a person's own assistant can suggest a thread.",
            ));
        }
        let who = named(&tx, job, &input.to).await?;
        let [(kind, id)] = who.as_slice() else {
            return Ok(invalid(if who.is_empty() {
                "No agent or person here has that name. Use their exact name or ID."
            } else {
                "More than one agent or person has that name. Use their ID."
            }));
        };
        if let Some(row) = query(
            &tx,
            "SELECT id FROM relay_work_threads WHERE workspace=$1 AND owner=$2 AND counterpart=$3 AND title=$4 AND state='suggested'",
            vec![job.workspace.clone().into(), job.requester.clone().into(), id.clone().into(), input.title.trim().into()],
        )
        .await?
        {
            tx.commit().await?;
            return Ok(json!({"state":"suggested","thread":row.try_get::<String>("", "id")?}));
        }
        let thread = NewThread {
            id: uuid::Uuid::new_v4().to_string(),
            kind: kind.clone(),
            counterpart: id.clone(),
            title: input.title.clone(),
            opening: input.opening.clone(),
        };
        if let Err(error) = save(
            &tx,
            &job.workspace,
            &job.requester,
            &job.agent,
            "assistant",
            &thread,
        )
        .await
        {
            if error.is::<sea_orm::DbErr>() {
                return Err(error);
            }
            return Ok(invalid(error.to_string()));
        }
        tx.commit().await?;
        Ok(json!({"state":"suggested","thread":thread.id,
            "message":"Saved as a suggestion. Nothing is sent until your person presses Start on it. Tell them in one line what you suggested and why."}))
    }

    /// `thread_reply`: one message in a running thread, sent as the member.
    pub(crate) async fn thread_reply(&self, job: &Job, input: &Reply) -> Result<Value> {
        let text = input.text.trim();
        if input.key.is_empty() || input.key.len() > 64 {
            return Ok(invalid("Use a stable key of 1 to 64 characters."));
        }
        if text.is_empty() || text.len() > 4000 {
            return Ok(invalid("Write a message of at most 4000 characters."));
        }
        let w = &job.workspace;
        let tx = self.workspace_tx(w).await?;
        super::super::super::control_actions::authorize(&tx, job, true).await?;
        if !for_owner(&tx, job).await? {
            return Ok(invalid(
                "Only a person's own assistant can answer in a thread.",
            ));
        }
        let Ok(thread) = locked(&tx, w, &job.requester, &input.thread).await else {
            return Ok(invalid(
                "That thread does not exist any more. Send nothing.",
            ));
        };
        if thread["agent"] != job.agent.as_str() || thread["state"] != "running" {
            return Ok(invalid(
                "This thread is not running. Send nothing; your person can resume it.",
            ));
        }
        if let Some(row) = query(
            &tx,
            "SELECT message FROM relay_work_thread_sends WHERE workspace=$1 AND thread=$2 AND key=$3",
            vec![w.clone().into(), input.thread.clone().into(), input.key.clone().into()],
        )
        .await?
        {
            tx.commit().await?;
            return Ok(json!({"state":"sent","message":row.try_get::<String>("", "message")?}));
        }
        if thread["sent"].as_i64().unwrap_or(0) >= i64::from(LIMIT) {
            stop(
                &tx,
                w,
                &input.thread,
                &format!(
                    "Stopped after {LIMIT} messages. Resume it to let your assistant continue."
                ),
            )
            .await?;
            tx.commit().await?;
            return Ok(invalid(
                "This thread reached its message limit and stopped. Tell your person where it stands.",
            ));
        }
        let agent = thread["kind"] == "agent";
        let message = NewMessage {
            id: uuid::Uuid::new_v4().to_string(),
            content: text.into(),
            agent: None,
            agents: if agent {
                vec![thread["counterpart"].as_str().unwrap_or_default().into()]
            } else {
                vec![]
            },
            thread: None,
            reply_to: None,
            attachments: vec![],
        };
        let chat = thread["chat"].as_str().context("Thread has no chat")?;
        if let Err(error) = self
            .as_owner_in(&tx, &person(&job.requester), w, chat, &message)
            .await
        {
            if error.is::<sea_orm::DbErr>() {
                return Err(error);
            }
            return Ok(invalid(format!("Not sent: {error}")));
        }
        mark(&tx, w, &message.id, &thread, "assistant").await?;
        execute(
            &tx,
            "INSERT INTO relay_work_thread_sends(workspace,thread,key,message) VALUES($1,$2,$3,$4)",
            vec![
                w.clone().into(),
                input.thread.clone().into(),
                input.key.clone().into(),
                message.id.clone().into(),
            ],
        )
        .await?;
        execute(
            &tx,
            "UPDATE relay_work_threads SET sent=sent+1,updated_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint WHERE workspace=$1 AND id=$2",
            vec![w.clone().into(), input.thread.clone().into()],
        )
        .await?;
        tx.commit().await?;
        Ok(json!({"state":"sent","message":message.id,
            "next":"You will be given their answer when it comes. Finish this attempt with one line for your person."}))
    }

    /// `finish_thread`: the matter is settled; the thread stops.
    pub(crate) async fn finish_thread(&self, job: &Job, input: &Finish) -> Result<Value> {
        let w = &job.workspace;
        let tx = self.workspace_tx(w).await?;
        super::super::super::control_actions::authorize(&tx, job, true).await?;
        if !for_owner(&tx, job).await? {
            return Ok(invalid(
                "Only a person's own assistant can finish a thread.",
            ));
        }
        let Ok(thread) = locked(&tx, w, &job.requester, &input.thread).await else {
            return Ok(invalid("That thread does not exist any more."));
        };
        if thread["agent"] == job.agent.as_str() && thread["state"] == "running" {
            let summary: String = input.summary.trim().chars().take(400).collect();
            stop(&tx, w, &input.thread, &format!("Finished: {summary}")).await?;
        }
        tx.commit().await?;
        Ok(json!({"state":"stopped"}))
    }

    /// Hands each running thread's new answers to its assistant, one run at a time.
    pub(crate) async fn dispatch_threads(&self) -> Result<usize> {
        if !self.pooled || !ready(&self.db).await? {
            return Ok(0);
        }
        let waiting = rows(
            &self.db,
            &format!("SELECT t.workspace,t.id FROM relay_work_threads t WHERE t.state='running' AND t.chat IS NOT NULL
               AND EXISTS({NEW} LIMIT 1)
               AND NOT EXISTS(SELECT 1 FROM relay_chat_jobs j WHERE j.workspace=t.workspace AND j.state<>'finished'
                 AND (j.reply=t.job OR j.payload->>'chat'=t.chat))
             ORDER BY t.updated_ms,t.id LIMIT 20"),
            vec![],
        )
        .await?;
        let mut admitted = 0;
        for row in waiting {
            admitted += usize::from(
                self.dispatch_thread(
                    &row.try_get::<String>("", "workspace")?,
                    &row.try_get::<String>("", "id")?,
                )
                .await?,
            );
        }
        Ok(admitted)
    }

    async fn dispatch_thread(&self, w: &str, id: &str) -> Result<bool> {
        let tx = self.workspace_tx(w).await?;
        let Some(thread) = json_rows(
            &tx,
            "SELECT to_jsonb(t) AS data FROM relay_work_threads t WHERE workspace=$1 AND id=$2 AND state='running' FOR UPDATE",
            vec![w.into(), id.into()],
        )
        .await?
        .into_iter()
        .next() else {
            return Ok(false);
        };
        let text = |key: &str| thread[key].as_str().unwrap_or_default().to_owned();
        let (owner, chat) = (text("owner"), text("chat"));
        let talk = rows(
            &tx,
            "SELECT m.sequence,m.content,m.kind=$3 AND m.author=$4 AS theirs,m.sequence>$5 AS fresh FROM (
               SELECT * FROM relay_chat_messages WHERE workspace=$1 AND chat=$2 AND thread IS NULL
                 AND state IN ('sent','complete') AND btrim(content)<>'' ORDER BY sequence DESC LIMIT 20) m ORDER BY m.sequence",
            vec![w.into(), chat.clone().into(), text("kind").into(), text("counterpart").into(), thread["handled"].as_i64().unwrap_or(0).into()],
        )
        .await?;
        let name: String = query(
            &tx,
            "SELECT CASE WHEN $2='agent' THEN (SELECT name FROM relay_shared_agents WHERE workspace=$1 AND id=$3)
               ELSE (SELECT COALESCE(NULLIF(btrim(display_name),''),'A workspace member') FROM relay_users WHERE id=$3) END AS name",
            vec![w.into(), text("kind").into(), text("counterpart").into()],
        )
        .await?
        .and_then(|row| row.try_get::<Option<String>>("", "name").ok().flatten())
        .unwrap_or_else(|| "them".into());
        let (mut handled, mut lines) = (thread["handled"].as_i64().unwrap_or(0), String::new());
        for row in &talk {
            let theirs: bool = row.try_get("", "theirs")?;
            if theirs && row.try_get::<bool>("", "fresh")? {
                handled = handled.max(row.try_get("", "sequence")?);
            }
            let said: String = row
                .try_get::<String>("", "content")?
                .chars()
                .take(2000)
                .collect();
            lines.push_str(&format!(
                "{}: {said}\n",
                if theirs { name.as_str() } else { "Your person" }
            ));
        }
        let title = text("title");
        let mut input = NewMessage {
            reply_to: None,
            id: uuid::Uuid::new_v4().to_string(),
            content: format!(
                "Thread · {title}\n{name} answered in the thread \"{title}\" (thread ID {id}). You speak for your person there until they stop it.\n\nThe chat so far, oldest first:\n{lines}\n{RULES}"
            ),
            agent: Some(text("agent")),
            agents: vec![],
            thread: None,
            attachments: vec![],
        };
        let intake = tx.begin().await?;
        let admitted = match super::super::run_place(
            &intake,
            w,
            &text("agent"),
            &owner,
            &format!("thread:{id}"),
            &format!("Thread: {title} · {name}"),
        )
        .await?
        {
            Some((room, notes)) => {
                input.thread = Some(notes);
                self.background_in(&intake, &person(&owner), w, &room, &input)
                    .await
            }
            None => Err(anyhow::anyhow!("The assistant's own chat is missing")),
        };
        match admitted {
            Ok(Some(job)) => {
                intake.commit().await?;
                execute(
                    &tx,
                    "UPDATE relay_work_threads SET handled=$3,job=$4,updated_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint WHERE workspace=$1 AND id=$2",
                    vec![w.into(), id.into(), handled.into(), job.reply.into()],
                )
                .await?;
                tx.commit().await?;
                Ok(true)
            }
            Ok(None) => {
                intake.rollback().await?;
                tx.commit().await?;
                Ok(false)
            }
            Err(error) => {
                intake.rollback().await?;
                if error.is::<sea_orm::DbErr>() {
                    return Err(error);
                }
                // Paused agents and a full queue pass: the answer waits. Anything else needs the member.
                if !(error.is::<AgentsPaused>()
                    || error.is::<QueueFull>()
                    || error.is::<BackgroundQueueFull>())
                {
                    stop(&tx, w, id, &format!("Stopped: {error}")).await?;
                }
                tx.commit().await?;
                Ok(false)
            }
        }
    }
}

/// SQL: a message from the other side of thread `t` its assistant has not been given.
const NEW: &str = "SELECT 1 FROM relay_chat_messages m WHERE m.workspace=t.workspace AND m.chat=t.chat AND m.sequence>t.handled
 AND m.kind=t.kind AND m.author=t.counterpart AND m.thread IS NULL AND m.state IN ('sent','complete') AND btrim(m.content)<>''";

/// What an assistant is told with every answer it is handed.
const RULES: &str = "What to do: answer with thread_reply. Your message is posted as your person's own message, so write as them, in the first person, in their tone; never say an assistant wrote it. Share only what this matter needs. If you need your person's decision, or information they did not agree to share, send nothing: say here what you need, and they will answer you here. When the matter is settled, call finish_thread with one line on how it ended. Then tell your person in one or two lines what happened.";

async fn stop(tx: &DatabaseTransaction, w: &str, id: &str, detail: &str) -> Result<()> {
    execute(
        tx,
        "UPDATE relay_work_threads SET state='stopped',detail=$3,updated_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint WHERE workspace=$1 AND id=$2",
        vec![w.into(), id.into(), detail.chars().take(400).collect::<String>().into()],
    )
    .await?;
    Ok(())
}

//! Assistant threads (schema 128).
//!
//! A thread is the permission a member gives their assistant to talk to one
//! agent or one person for them about one matter, until they stop it.
//!
//! - The assistant only suggests a thread; the member starts it.
//! - What the assistant sends is an ordinary message from the member in their
//!   direct chat with that agent or person. Nobody else can tell who wrote it:
//!   `assisted` on the message is shown to the member (and kept for admins).
//! - While a thread runs, each answer from the other side goes to the assistant
//!   (`runs.rs`), which answers on its own. The member can write in the chat
//!   themselves at any time, stop the thread, resume it or delete it.
//! - One running thread per member and chat. A stopped thread sends nothing.
//! - Deleting a thread removes the assistant's notes about it; the messages
//!   already sent stay in the chat, as the member's own.
use super::super::*;
use crate::identity::chat::model::{Job, NewChat, NewMessage};
use serde::Deserialize;
use serde_json::json;

#[path = "assistant_thread_runs.rs"]
pub(crate) mod runs;
#[path = "assistant_thread_takeover.rs"]
pub(crate) mod takeover;
#[cfg(test)]
#[path = "assistant_threads_tests.rs"]
mod tests;

/// Messages an assistant sends in one thread before it stops and asks.
pub(crate) const LIMIT: i32 = 30;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewThread {
    pub id: String,
    /// `agent` or `user`.
    pub kind: String,
    pub counterpart: String,
    pub title: String,
    pub opening: String,
}

pub(in crate::identity::postgres) async fn ready(db: &impl ConnectionTrait) -> Result<bool> {
    Ok(query(
        db,
        "SELECT to_regclass('relay_work_threads') IS NOT NULL AS ready",
        vec![],
    )
    .await?
    .context("Missing schema check")?
    .try_get("", "ready")?)
}

fn person(id: &str) -> User {
    User {
        id: id.into(),
        email: String::new(),
        display_name: None,
    }
}

/// The name of the agent or person a thread is with, when the member may have one with them.
async fn counterpart(
    tx: &DatabaseTransaction,
    w: &str,
    owner: &str,
    kind: &str,
    id: &str,
) -> Result<String> {
    if kind == "agent" {
        let row = query(
            tx,
            "SELECT name FROM relay_shared_agents WHERE workspace=$1 AND id=$2",
            vec![w.into(), id.into()],
        )
        .await?
        .context("Agent is not shared with you")?;
        ensure!(
            !super::is_assistant(tx, w, id).await?,
            "An assistant talks only to its own person"
        );
        ensure!(
            super::super::chat::agent_access(tx, owner, w, id).await?,
            "Agent is not shared with you"
        );
        return Ok(row.try_get("", "name")?);
    }
    ensure!(kind == "user", "A thread is with a person or an agent");
    ensure!(id != owner, "Start a thread with someone else");
    let row = query(
        tx,
        "SELECT COALESCE(NULLIF(btrim(u.display_name),''),'A workspace member') AS name
         FROM relay_members m JOIN relay_users u ON u.id=m.user_id WHERE m.workspace=$1 AND m.user_id=$2",
        vec![w.into(), id.into()],
    )
    .await?
    .context("This person is not in the workspace")?;
    Ok(row.try_get("", "name")?)
}

/// Saves a suggested thread. The same ID saves once.
pub(super) async fn save(
    tx: &DatabaseTransaction,
    w: &str,
    owner: &str,
    agent: &str,
    origin: &str,
    input: &NewThread,
) -> Result<String> {
    ensure!(
        uuid::Uuid::parse_str(&input.id).is_ok(),
        "Invalid thread ID"
    );
    let (title, opening) = (input.title.trim(), input.opening.trim());
    ensure!(
        !title.is_empty() && title.chars().count() <= 120,
        "Give the thread a short title"
    );
    ensure!(
        !opening.is_empty() && opening.len() <= 4000,
        "Write the first message, at most 4000 characters"
    );
    let name = counterpart(tx, w, owner, &input.kind, &input.counterpart).await?;
    execute(
        tx,
        "INSERT INTO relay_work_threads(workspace,id,owner,agent,kind,counterpart,title,opening,origin,state)
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,'suggested') ON CONFLICT(workspace,id) DO NOTHING",
        vec![
            w.into(),
            input.id.clone().into(),
            owner.into(),
            agent.into(),
            input.kind.clone().into(),
            input.counterpart.clone().into(),
            title.into(),
            opening.into(),
            origin.into(),
        ],
    )
    .await?;
    Ok(name)
}

/// The private record on a message a thread sent: shown to its author only.
pub(super) async fn mark(
    tx: &DatabaseTransaction,
    w: &str,
    message: &str,
    thread: &Value,
    by: &str,
) -> Result<()> {
    execute(
        tx,
        "UPDATE relay_chat_messages SET assisted=$3 WHERE workspace=$1 AND id=$2 AND assisted IS NULL",
        vec![
            w.into(),
            message.into(),
            json!({"agent":thread["agent"],"thread":thread["id"],"title":thread["title"],"by":by})
                .into(),
        ],
    )
    .await?;
    Ok(())
}

/// The member's thread, locked for this transaction.
pub(super) async fn locked(
    tx: &DatabaseTransaction,
    w: &str,
    owner: &str,
    id: &str,
) -> Result<Value> {
    json_rows(
        tx,
        "SELECT to_jsonb(t) AS data FROM relay_work_threads t WHERE workspace=$1 AND id=$2 AND owner=$3 AND state<>'deleted' FOR UPDATE",
        vec![w.into(), id.into(), owner.into()],
    )
    .await?
    .into_iter()
    .next()
    .context("Thread not found")
}

impl PgStore {
    /// The member's threads, newest first, with who each is with.
    pub async fn threads(&self, user: &User, w: &str) -> Result<Value> {
        let tx = self.workspace_tx(w).await?;
        role(&tx, w, &user.id).await?;
        if !ready(&tx).await? {
            return Ok(json!({"threads":[]}));
        }
        let threads = json_rows(
            &tx,
            "SELECT jsonb_build_object('id',t.id,'kind',t.kind,'counterpart',t.counterpart,'title',t.title,'opening',t.opening,
               'instruction',t.instruction,'anchor',t.anchor,'created_ms',t.created_ms,'chat',t.chat,'state',t.state,'detail',t.detail,'sent',t.sent,'origin',t.origin,'updated_ms',t.updated_ms,
               'name',CASE WHEN t.kind='agent' THEN (SELECT a.name FROM relay_shared_agents a WHERE a.workspace=t.workspace AND a.id=t.counterpart)
                 ELSE (SELECT COALESCE(NULLIF(btrim(u.display_name),''),'A workspace member') FROM relay_users u WHERE u.id=t.counterpart) END,
               'working',EXISTS(SELECT 1 FROM relay_chat_jobs j WHERE j.workspace=t.workspace AND j.reply=t.job AND j.state<>'finished'),
               'notes',(SELECT jsonb_build_object('chat',r.chat,'thread',r.thread) FROM relay_assistant_threads r
                 WHERE r.workspace=t.workspace AND r.agent=t.agent AND r.automation='thread:'||t.id)) AS data
             FROM relay_work_threads t WHERE t.workspace=$1 AND t.owner=$2 AND t.state<>'deleted'
             ORDER BY t.updated_ms DESC,t.id LIMIT 100",
            vec![w.into(), user.id.clone().into()],
        )
        .await?;
        tx.commit().await?;
        Ok(json!({"threads":threads}))
    }

    /// A thread the member writes themselves: saved and started at once.
    pub async fn thread_create(
        &self,
        user: &User,
        w: &str,
        input: &NewThread,
    ) -> Result<(String, Option<Job>)> {
        let agent = self
            .assistant_of(user, w)
            .await?
            .context("You have no assistant yet")?;
        let tx = self.workspace_tx(w).await?;
        ensure!(ready(&tx).await?, "Threads are not available yet");
        role(&tx, w, &user.id).await?;
        save(&tx, w, &user.id, &agent, "owner", input).await?;
        tx.commit().await?;
        self.thread_start(user, w, &input.id).await
    }

    /// Start a suggested thread, or resume a stopped one. The first start sends
    /// the opening message as the member, in their chat with the other side.
    pub async fn thread_start(
        &self,
        user: &User,
        w: &str,
        id: &str,
    ) -> Result<(String, Option<Job>)> {
        let tx = self.workspace_tx(w).await?;
        ensure!(ready(&tx).await?, "Threads are not available yet");
        let thread = locked(&tx, w, &user.id, id).await?;
        let kind = thread["kind"].as_str().unwrap_or_default();
        let other = thread["counterpart"].as_str().unwrap_or_default();
        let name = counterpart(&tx, w, &user.id, kind, other).await?;
        if thread["state"] == "running" {
            tx.commit().await?;
            return Ok((thread["chat"].as_str().unwrap_or_default().into(), None));
        }
        let first = thread["chat"].is_null();
        let chat = match thread["chat"].as_str() {
            Some(chat) => chat.to_owned(),
            None => {
                let direct = NewChat {
                    id: uuid::Uuid::new_v4().to_string(),
                    name: if kind == "agent" {
                        name.clone()
                    } else {
                        "Direct message".into()
                    },
                    users: if kind == "user" {
                        vec![other.into()]
                    } else {
                        vec![]
                    },
                    agents: if kind == "agent" {
                        vec![other.into()]
                    } else {
                        vec![]
                    },
                    kind: "direct".into(),
                    description: String::new(),
                };
                super::super::chat::chat_create_in(&tx, user, w, &direct).await?
            }
        };
        ensure!(
            query(
                &tx,
                "SELECT 1 AS found FROM relay_work_threads WHERE workspace=$1 AND owner=$2 AND chat=$3 AND state='running' AND id<>$4",
                vec![w.into(), user.id.clone().into(), chat.clone().into(), id.into()],
            )
            .await?
            .is_none(),
            "You already have a running thread with {name}. Stop it before starting another."
        );
        let mut jobs = vec![];
        if first {
            // The thread's ID is its opening message's ID: a retried start sends once.
            let opening = NewMessage {
                id: id.into(),
                content: thread["opening"].as_str().unwrap_or_default().into(),
                agent: None,
                agents: if kind == "agent" {
                    vec![other.into()]
                } else {
                    vec![]
                },
                thread: None,
                reply_to: None,
                attachments: vec![],
            };
            jobs = self.as_owner_in(&tx, user, w, &chat, &opening).await?;
            let by = if thread["origin"] == "assistant" {
                "assistant"
            } else {
                "owner"
            };
            mark(&tx, w, id, &thread, by).await?;
        }
        execute(
            &tx,
            "UPDATE relay_work_threads t SET state='running',detail='',chat=$3,sent=sent+$4,
               handled=GREATEST(handled,COALESCE((SELECT m.sequence FROM relay_chat_messages m WHERE m.workspace=$1 AND m.id=$2),0)),
               updated_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint
             WHERE workspace=$1 AND id=$2",
            vec![w.into(), id.into(), chat.clone().into(), i32::from(first).into()],
        )
        .await?;
        audit_with_summary(
            &tx,
            &user.id,
            Some(w),
            if first {
                "assistant_thread_started"
            } else {
                "assistant_thread_resumed"
            },
            id,
            &format!(
                "{} a thread with {name}: {}",
                if first { "Started" } else { "Resumed" },
                thread["title"].as_str().unwrap_or_default()
            ),
        )
        .await?;
        tx.commit().await?;
        Ok((chat, jobs.into_iter().next()))
    }

    /// Stop, resume or delete a thread.
    pub async fn thread_set(
        &self,
        user: &User,
        w: &str,
        id: &str,
        state: &str,
    ) -> Result<Option<Job>> {
        if state == "running" {
            return Ok(self.thread_start(user, w, id).await?.1);
        }
        ensure!(
            ["stopped", "deleted"].contains(&state),
            "Unknown thread state"
        );
        let tx = self.workspace_tx(w).await?;
        ensure!(ready(&tx).await?, "Threads are not available yet");
        let thread = locked(&tx, w, &user.id, id).await?;
        if state == "stopped" {
            ensure!(thread["state"] == "running", "This thread is not running");
        } else {
            forget(&tx, w, &thread).await?;
        }
        execute(
            &tx,
            "UPDATE relay_work_threads SET state=$3,detail=$4,updated_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint WHERE workspace=$1 AND id=$2",
            vec![
                w.into(),
                id.into(),
                state.into(),
                if state == "stopped" { "You stopped it." } else { "" }.into(),
            ],
        )
        .await?;
        audit_with_summary(
            &tx,
            &user.id,
            Some(w),
            &format!("assistant_thread_{state}"),
            id,
            &format!(
                "{} a thread: {}",
                if state == "stopped" {
                    "Stopped"
                } else {
                    "Deleted"
                },
                thread["title"].as_str().unwrap_or_default()
            ),
        )
        .await?;
        tx.commit().await?;
        Ok(None)
    }
}

/// Removes the assistant's notes about a thread from its room: the line that
/// names it and everything under it. Work still running there is left to end.
async fn forget(tx: &DatabaseTransaction, w: &str, thread: &Value) -> Result<()> {
    let args: Vec<sea_orm::Value> = vec![
        w.into(),
        thread["agent"].as_str().unwrap_or_default().into(),
        format!("thread:{}", thread["id"].as_str().unwrap_or_default()).into(),
    ];
    execute(
        tx,
        "UPDATE relay_chat_messages m SET content='',attachments='[]',state='deleted'
         FROM relay_assistant_threads r
         WHERE r.workspace=$1 AND r.agent=$2 AND r.automation=$3 AND m.workspace=r.workspace AND m.chat=r.chat
           AND (m.id=r.thread OR m.thread=r.thread) AND m.state NOT IN ('queued','running')",
        args.clone(),
    )
    .await?;
    execute(
        tx,
        "DELETE FROM relay_assistant_threads WHERE workspace=$1 AND agent=$2 AND automation=$3",
        args,
    )
    .await?;
    Ok(())
}

pub(crate) use migration::AssistantThreads;
mod migration {
    use sea_orm_migration::prelude::*;
    pub(crate) struct AssistantThreads;
    impl MigrationName for AssistantThreads {
        fn name(&self) -> &str {
            "m20261117_000128_assistant_threads"
        }
    }
    #[async_trait::async_trait]
    impl MigrationTrait for AssistantThreads {
        async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
            m.get_connection()
                .execute_unprepared(include_str!("assistant_threads.sql"))
                .await?;
            Ok(())
        }
        async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
            Err(DbErr::Custom(
                "Assistant threads require a forward migration".into(),
            ))
        }
    }
}

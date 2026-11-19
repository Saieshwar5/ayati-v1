//! Handing a chat over from a message (schema 136).
//!
//! The member picks a message the other side sent in a direct chat and says in
//! one instruction what their assistant should do. The thread runs at once in
//! that chat. Nothing is sent for the member first: the assistant is handed
//! that message and everything after it, and writes the first reply itself.
use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TakeOver {
    pub id: String,
    pub chat: String,
    /// The message the assistant answers first.
    pub message: String,
    pub instruction: String,
}

/// The instruction on one line, short enough for lists and the Audit log.
fn label(instruction: &str) -> String {
    let line = instruction.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= 80 {
        return line;
    }
    format!("{}…", line.chars().take(79).collect::<String>())
}

impl PgStore {
    /// Starts a running thread in `chat` from one of the other side's messages.
    /// The same ID starts once.
    pub async fn thread_take_over(&self, user: &User, w: &str, input: &TakeOver) -> Result<String> {
        let agent = self
            .assistant_of(user, w)
            .await?
            .context("You have no assistant yet")?;
        ensure!(
            uuid::Uuid::parse_str(&input.id).is_ok(),
            "Invalid thread ID"
        );
        let instruction = input.instruction.trim();
        ensure!(
            !instruction.is_empty() && instruction.chars().count() <= 1000,
            "Say what your assistant should do, in at most 1000 characters"
        );
        let tx = self.workspace_tx(w).await?;
        ensure!(ready(&tx).await?, "Threads are not available yet");
        if query(
            &tx,
            "SELECT 1 AS found FROM relay_work_threads WHERE workspace=$1 AND id=$2 AND owner=$3 AND chat=$4",
            vec![w.into(), input.id.clone().into(), user.id.clone().into(), input.chat.clone().into()],
        )
        .await?
        .is_some()
        {
            tx.commit().await?;
            return Ok(input.chat.clone());
        }
        super::super::super::chat::access(&tx, user, w, &input.chat).await?;
        // A direct chat has two sides; the message must be the other side's own.
        let from = query(
            &tx,
            "SELECT m.sequence,m.kind,m.author FROM relay_chat_messages m
               JOIN relay_chats c ON c.workspace=m.workspace AND c.id=m.chat
             WHERE m.workspace=$1 AND m.chat=$2 AND m.id=$3 AND c.kind='direct' AND NOT c.work AND m.thread IS NULL
               AND m.state IN ('sent','complete')
               AND (SELECT count(*) FROM relay_chat_people p WHERE p.workspace=c.workspace AND p.chat=c.id)
                 +(SELECT count(*) FROM relay_chat_agents a WHERE a.workspace=c.workspace AND a.chat=c.id)=2
               AND CASE m.kind
                 WHEN 'agent' THEN EXISTS(SELECT 1 FROM relay_chat_agents a WHERE a.workspace=c.workspace AND a.chat=c.id AND a.agent=m.author)
                 WHEN 'user' THEN m.author<>$4 AND EXISTS(SELECT 1 FROM relay_chat_people p WHERE p.workspace=c.workspace AND p.chat=c.id AND p.user_id=m.author)
                 ELSE false END",
            vec![w.into(), input.chat.clone().into(), input.message.clone().into(), user.id.clone().into()],
        )
        .await?
        .context("Choose a message the other side sent in a direct chat")?;
        let (kind, other): (String, String) =
            (from.try_get("", "kind")?, from.try_get("", "author")?);
        let name = counterpart(&tx, w, &user.id, &kind, &other).await?;
        ensure!(
            query(
                &tx,
                "SELECT 1 AS found FROM relay_work_threads WHERE workspace=$1 AND owner=$2 AND chat=$3 AND state='running'",
                vec![w.into(), user.id.clone().into(), input.chat.clone().into()],
            )
            .await?
            .is_none(),
            "Your assistant is already replying to {name} here. Stop it before starting again."
        );
        let title = label(instruction);
        // `handled` sits just before the chosen message, so it is the first one handed over.
        execute(
            &tx,
            "INSERT INTO relay_work_threads(workspace,id,owner,agent,kind,counterpart,title,opening,instruction,anchor,chat,origin,state,handled)
             VALUES($1,$2,$3,$4,$5,$6,$7,'',$8,$9,$10,'owner','running',$11)",
            vec![
                w.into(),
                input.id.clone().into(),
                user.id.clone().into(),
                agent.into(),
                kind.into(),
                other.into(),
                title.clone().into(),
                instruction.into(),
                input.message.clone().into(),
                input.chat.clone().into(),
                (from.try_get::<i64>("", "sequence")? - 1).into(),
            ],
        )
        .await?;
        audit_with_summary(
            &tx,
            &user.id,
            Some(w),
            "assistant_thread_started",
            &input.id,
            &format!("Handed the chat with {name} to their assistant: {title}"),
        )
        .await?;
        // No new message wakes the scheduler here, so say there is work.
        query(
            &tx,
            "SELECT pg_notify('relay_work', current_schema() || '/schedule')::text AS sent",
            vec![],
        )
        .await?;
        tx.commit().await?;
        Ok(input.chat.clone())
    }
}

pub(crate) use migration::ThreadTakeOver;
mod migration {
    use sea_orm_migration::prelude::*;
    pub(crate) struct ThreadTakeOver;
    impl MigrationName for ThreadTakeOver {
        fn name(&self) -> &str {
            "m20261119_000136_assistant_thread_takeover"
        }
    }
    #[async_trait::async_trait]
    impl MigrationTrait for ThreadTakeOver {
        async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
            m.get_connection()
                .execute_unprepared(include_str!("assistant_thread_takeover.sql"))
                .await?;
            Ok(())
        }
        async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
            Err(DbErr::Custom(
                "Thread take-over requires a forward migration".into(),
            ))
        }
    }
}

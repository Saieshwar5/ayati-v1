//! One position per chat for a member's assistant (schema 141).
//!
//! In a direct chat with one person or one agent, the member chooses what
//! their assistant does there:
//! - **off**: nothing, unless they ask for a draft;
//! - **ask**: each time the other side writes, the assistant writes the reply
//!   in its own room and it waits for the member, who sends it themselves;
//! - **auto**: the assistant sends replies as the member (`runs.rs`), and goes
//!   back to `ask` by itself when the matter is settled or after `LIMIT`
//!   sent messages.
//!
//! A chat that is on is a running thread; `mode` says which of the two. This
//! replaces handing a chat over from a message (schema 136), whose columns
//! `instruction` and `anchor` are still used.
use super::*;

#[cfg(test)]
#[path = "assistant_reply_modes_tests.rs"]
mod tests;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetReplies {
    /// The thread's ID if this turns the assistant on in a chat for the first time.
    pub id: String,
    pub chat: String,
    /// `off`, `ask` or `auto`.
    pub mode: String,
    /// What the assistant should do there. Left out, the earlier one is kept.
    #[serde(default)]
    pub instruction: Option<String>,
}

/// The instruction on one line, short enough for lists and the Audit log.
fn label(instruction: &str) -> String {
    let line = instruction.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= 80 {
        return line;
    }
    format!("{}…", line.chars().take(79).collect::<String>())
}

/// The other side of a direct chat the member is in: (kind, id), when there is exactly one.
async fn other_side(
    tx: &DatabaseTransaction,
    w: &str,
    chat: &str,
    owner: &str,
) -> Result<(String, String)> {
    let found = rows(
        tx,
        "SELECT o.kind,o.id FROM relay_chats c
           JOIN relay_chat_people me ON me.workspace=c.workspace AND me.chat=c.id AND me.user_id=$3
           CROSS JOIN LATERAL (
             SELECT 'user' AS kind,p.user_id AS id FROM relay_chat_people p WHERE p.workspace=c.workspace AND p.chat=c.id AND p.user_id<>$3
             UNION ALL SELECT 'agent',a.agent FROM relay_chat_agents a WHERE a.workspace=c.workspace AND a.chat=c.id) o
         WHERE c.workspace=$1 AND c.id=$2 AND c.kind='direct'",
        vec![w.into(), chat.into(), owner.into()],
    )
    .await?;
    let [row] = found.as_slice() else {
        anyhow::bail!("Your assistant replies only in a direct chat with one person or one agent");
    };
    Ok((row.try_get("", "kind")?, row.try_get("", "id")?))
}

impl PgStore {
    /// Sets what the member's assistant does in `chat`. Returns the thread that
    /// is on there, if any. Nothing is sent for the member by this.
    pub async fn thread_position(
        &self,
        user: &User,
        w: &str,
        input: &SetReplies,
    ) -> Result<Option<String>> {
        let agent = self
            .assistant_of(user, w)
            .await?
            .context("You have no assistant yet")?;
        ensure!(
            ["off", "ask", "auto"].contains(&input.mode.as_str()),
            "Choose off, ask me first or reply for me"
        );
        ensure!(
            uuid::Uuid::parse_str(&input.id).is_ok(),
            "Invalid thread ID"
        );
        let instruction = input.instruction.as_deref().map(str::trim);
        ensure!(
            instruction.is_none_or(|text| text.chars().count() <= 1000),
            "Say what your assistant should do, in at most 1000 characters"
        );
        let tx = self.workspace_tx(w).await?;
        ensure!(ready(&tx).await?, "Threads are not available yet");
        super::super::super::chat::access(&tx, user, w, &input.chat).await?;
        let (kind, other) = other_side(&tx, w, &input.chat, &user.id).await?;
        let name = counterpart(&tx, w, &user.id, &kind, &other).await?;
        let existing = json_rows(
            &tx,
            "SELECT to_jsonb(t) AS data FROM relay_work_threads t
             WHERE workspace=$1 AND owner=$2 AND chat=$3 AND state IN ('running','stopped')
             ORDER BY (state='running') DESC,updated_ms DESC,id LIMIT 1 FOR UPDATE",
            vec![w.into(), user.id.clone().into(), input.chat.clone().into()],
        )
        .await?
        .into_iter()
        .next();
        let running = existing.as_ref().is_some_and(|t| t["state"] == "running");
        if input.mode == "off" {
            if let (true, Some(thread)) = (running, &existing) {
                let id = thread["id"].as_str().unwrap_or_default();
                execute(
                    &tx,
                    "UPDATE relay_work_threads SET state='stopped',detail='',draft=NULL,updated_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint WHERE workspace=$1 AND id=$2",
                    vec![w.into(), id.into()],
                )
                .await?;
                audit_with_summary(
                    &tx,
                    &user.id,
                    Some(w),
                    "assistant_thread_stopped",
                    id,
                    &format!("Turned their assistant off in the chat with {name}"),
                )
                .await?;
            }
            tx.commit().await?;
            return Ok(None);
        }
        let same = running
            && existing
                .as_ref()
                .is_some_and(|t| t["mode"] == input.mode.as_str());
        let id = existing
            .as_ref()
            .and_then(|t| t["id"].as_str())
            .unwrap_or(&input.id)
            .to_owned();
        let title = match (instruction, &existing) {
            (Some(text), _) if !text.is_empty() => label(text),
            (None, Some(thread)) => thread["title"].as_str().unwrap_or_default().to_owned(),
            _ => format!("Chat with {name}"),
        };
        if existing.is_none() {
            execute(
                &tx,
                "INSERT INTO relay_work_threads(workspace,id,owner,agent,kind,counterpart,title,opening,chat,origin,state,mode)
                 VALUES($1,$2,$3,$4,$5,$6,$7,'',$8,'owner','running',$9)",
                vec![
                    w.into(),
                    id.clone().into(),
                    user.id.clone().into(),
                    agent.into(),
                    kind.clone().into(),
                    other.clone().into(),
                    title.clone().into(),
                    input.chat.clone().into(),
                    input.mode.clone().into(),
                ],
            )
            .await?;
        }
        // A change of position starts again from the member's own last message, so
        // what the other side wrote since is the first thing handed over. `anchor`
        // is that first message, where replies sent for the member begin.
        execute(
            &tx,
            "UPDATE relay_work_threads t SET state='running',mode=$3,detail='',title=$4,
               instruction=COALESCE($5,t.instruction),
               sent=CASE WHEN $6 OR $3<>'auto' THEN t.sent ELSE 0 END,
               draft=CASE WHEN $6 THEN t.draft ELSE NULL END,
               handled=CASE WHEN $6 THEN t.handled ELSE own.last END,
               anchor=CASE WHEN $6 THEN t.anchor WHEN $3='auto' THEN (SELECT m.id FROM relay_chat_messages m
                   WHERE m.workspace=t.workspace AND m.chat=t.chat AND m.sequence>own.last AND m.kind=t.kind AND m.author=t.counterpart
                     AND m.thread IS NULL AND m.state IN ('sent','complete') ORDER BY m.sequence LIMIT 1) END,
               updated_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint
             FROM (SELECT COALESCE(max(m.sequence),0) AS last FROM relay_chat_messages m
               WHERE m.workspace=$1 AND m.chat=$7 AND m.kind='user' AND m.author=$8 AND m.thread IS NULL) own
             WHERE t.workspace=$1 AND t.id=$2",
            vec![
                w.into(),
                id.clone().into(),
                input.mode.clone().into(),
                title.clone().into(),
                instruction.map(str::to_owned).into(),
                same.into(),
                input.chat.clone().into(),
                user.id.clone().into(),
            ],
        )
        .await?;
        if !same {
            let does = if input.mode == "auto" {
                "reply for them"
            } else {
                "write replies for them to send"
            };
            audit_with_summary(
                &tx,
                &user.id,
                Some(w),
                if running {
                    "assistant_thread_mode"
                } else {
                    "assistant_thread_started"
                },
                &id,
                &format!("Let their assistant {does} in the chat with {name}: {title}"),
            )
            .await?;
        }
        // No new message wakes the scheduler here, so say there is work.
        query(
            &tx,
            "SELECT pg_notify('relay_work', current_schema() || '/schedule')::text AS sent",
            vec![],
        )
        .await?;
        tx.commit().await?;
        Ok(Some(id))
    }

    /// The member does not want the reply that waits for them in a thread.
    pub async fn thread_draft_discard(&self, user: &User, w: &str, id: &str) -> Result<()> {
        let tx = self.workspace_tx(w).await?;
        ensure!(ready(&tx).await?, "Threads are not available yet");
        locked(&tx, w, &user.id, id).await?;
        execute(
            &tx,
            "UPDATE relay_work_threads SET draft=NULL WHERE workspace=$1 AND id=$2",
            vec![w.into(), id.into()],
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
}

pub(crate) use migration::ReplyModes;
mod migration {
    use sea_orm_migration::prelude::*;
    pub(crate) struct ReplyModes;
    impl MigrationName for ReplyModes {
        fn name(&self) -> &str {
            "m20261121_000141_assistant_reply_modes"
        }
    }
    #[async_trait::async_trait]
    impl MigrationTrait for ReplyModes {
        async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
            m.get_connection()
                .execute_unprepared(include_str!("assistant_reply_modes.sql"))
                .await?;
            Ok(())
        }
        async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
            Err(DbErr::Custom(
                "Assistant reply modes require a forward migration".into(),
            ))
        }
    }
}

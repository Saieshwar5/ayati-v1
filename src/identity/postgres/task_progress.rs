//! Read existing work through its source conversation, without granting new work.
use super::*;
use serde_json::json;
impl PgStore {
    pub(crate) async fn task_progress(
        &self,
        user: &User,
        w: &str,
        chat: &str,
        reply: &str,
    ) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        super::chat::access(&tx, user, w, chat).await?;
        let row = query(
            &tx,
            "SELECT j.payload FROM relay_chat_jobs j JOIN relay_chat_messages m ON m.workspace=j.workspace AND m.id=j.reply WHERE j.workspace=$1 AND m.chat=$2 AND j.reply=$3 AND m.kind='agent'",
            vec![w.into(), chat.into(), reply.into()],
        )
        .await?
        .context("Task unavailable")?;
        let job: crate::identity::chat::store::Job =
            serde_json::from_value(row.try_get("", "payload")?)?;
        ensure!(
            job.workspace == w && job.chat == chat && job.reply == reply,
            "Task scope mismatch"
        );
        // Visibility is independent of agent request permission and maintainership.
        super::approval_authority::agent_visible(&tx, &user.id, w, &job.agent).await?;
        let row = query(
            &tx,
            include_str!("agent_requests.sql"),
            vec![
                w.into(),
                job.agent.clone().into(),
                user.id.clone().into(),
                false.into(),
                false.into(),
                i64::MAX.into(),
                reply.into(),
            ],
        )
        .await?
        .context("Task unavailable")?;
        let mut data = super::agent_requests::project(&tx, user, w, &job.agent, row)
            .await?
            .context("Task source unavailable")?;
        ensure!(data["chat"] == chat, "Task conversation mismatch");
        let mut messages = vec![json!({"id":reply,"kind":"agent"})];
        super::question_views::hydrate(&tx, user, w, &mut messages).await?;
        data["questions"] = messages[0]
            .get("questions")
            .cloned()
            .unwrap_or_else(|| json!([]));
        data["checked_ms"] = json!(
            query(
                &tx,
                "SELECT (extract(epoch FROM clock_timestamp())*1000)::bigint AS checked",
                vec![]
            )
            .await?
            .unwrap()
            .try_get::<i64>("", "checked")?
        );
        tx.commit().await?;
        Ok(data)
    }
}

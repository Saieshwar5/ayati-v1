//! The agent's queued reply and job for an accepted message.
use super::*;

impl PgStore {
    /// The agent's own queued reply and job for an accepted message, with its
    /// delivered tick, and a front-desk answer when it is busy.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn queue_reply(
        &self,
        tx: &DatabaseTransaction,
        user: &User,
        workspace: &str,
        chat: &str,
        input: &NewMessage,
        agent: &str,
        class: WorkClass<'_>,
    ) -> Result<Job> {
        let reply = uuid::Uuid::new_v4().to_string();
        let kind: String = query(
            tx,
            "SELECT kind FROM relay_chats WHERE workspace=$1 AND id=$2",
            vec![workspace.into(), chat.into()],
        )
        .await?
        .context("Conversation missing")?
        .try_get("", "kind")?;
        let managed = query(
            tx,
            "SELECT settings FROM relay_shared_agents WHERE workspace=$1 AND id=$2 AND revision>0",
            vec![workspace.into(), agent.into()],
        )
        .await?;
        let settings: Option<Value> = managed.map(|r| r.try_get("", "settings")).transpose()?;
        let context_limit: i64 = if settings.is_some() { 60 } else { 30 };
        let context_thread = if settings.is_some() {
            None
        } else {
            input
                .thread
                .as_deref()
                .or_else(|| (kind != "direct").then_some(input.id.as_str()))
        };
        let since_join = settings
            .as_ref()
            .is_some_and(|s| s["history"] == "since_join");
        let messages = super::conversation_context::recent(
            tx,
            super::conversation_context::RecentScope {
                workspace,
                chat,
                agent,
                thread: context_thread,
                main_only: false,
                since_join,
                limit: context_limit,
                before: i64::MAX,
                request: &input.id,
            },
        )
        .await?;
        // In a channel or group several people read, the answer goes in a thread
        // under the person's message unless the channel answers in the channel.
        let reply_thread = super::channel_threads::reply_thread(
            tx,
            workspace,
            chat,
            &input.id,
            input.thread.as_deref(),
            class == WorkClass::Interactive,
        )
        .await?;
        execute(tx,"INSERT INTO relay_chat_messages(id,workspace,chat,author,kind,content,state,at,requester,thread) VALUES($1,$2,$3,$4,'agent','','queued',$5,$6,$7)",vec![reply.clone().into(),workspace.into(),chat.into(),agent.into(),now().into(),user.id.clone().into(),reply_thread.into()]).await?;
        let job = Job {
            claim: None,
            reply,
            chat: chat.into(),
            workspace: workspace.into(),
            requester: user.id.clone(),
            agent: agent.into(),
            prompt: super::conversation_context::prompt(&input.id, messages)?,
        };
        execute(
            tx,
            "INSERT INTO relay_chat_jobs(workspace,reply,payload,dispatch,run_seconds,work_class,request_message) VALUES($1,$2,$3,$4,$5,$6,$7)",
            vec![
                workspace.into(),
                job.reply.clone().into(),
                serde_json::to_value(&job)?.into(),
                if self.pooled { "pooled" } else { "polling" }.into(),
                (if self.pooled { self.assignment_seconds as i32 } else { 180 }).into(),
                class.as_str().into(),
                input.id.clone().into(),
            ],
        )
        .await?;
        super::agent_receipts::delivered(tx, workspace, chat, &input.id, &job.agent).await?;
        // A person's request that queues behind the agent's other work gets a
        // short answer from the front desk while the work lane stays busy.
        if class == WorkClass::Interactive {
            super::chat::front_desk::enqueue_if_busy(tx, self.pooled, user, &job, input).await?;
        }
        Ok(job)
    }
}

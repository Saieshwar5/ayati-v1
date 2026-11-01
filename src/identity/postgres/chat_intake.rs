//! Server-owned request classification; shared authorization and idempotent intake.
use super::{
    chat::{access, agent_access_in},
    *,
};
use crate::identity::chat::model::{Job, NewMessage};
use serde_json::json;

#[derive(Clone, Copy, PartialEq)]
enum WorkClass<'a> {
    Interactive,
    Communication,
    Background,
    Handoff(&'a str),
    /// A person's question the agent moved from a shared room to their direct
    /// conversation: written by that agent, answered for the same person.
    Private(&'a str),
}
impl WorkClass<'_> {
    fn as_str(self) -> &'static str {
        match self {
            Self::Interactive | Self::Communication | Self::Private(_) => "interactive",
            Self::Background | Self::Handoff(_) => "background",
        }
    }
    fn background(self) -> bool {
        !matches!(
            self,
            Self::Interactive | Self::Communication | Self::Private(_)
        )
    }
}
impl PgStore {
    pub async fn chat_send(
        &self,
        user: &User,
        workspace: &str,
        chat: &str,
        input: &NewMessage,
    ) -> Result<Option<Job>> {
        self.enqueue_chat(user, workspace, chat, input, WorkClass::Interactive)
            .await
    }
    pub(crate) async fn chat_background(
        &self,
        user: &User,
        workspace: &str,
        chat: &str,
        input: &NewMessage,
    ) -> Result<Option<Job>> {
        ensure!(
            self.pooled && input.agent.is_some(),
            "Background requests require pooled execution and an agent"
        );
        self.enqueue_chat(user, workspace, chat, input, WorkClass::Background)
            .await
    }
    async fn enqueue_chat(
        &self,
        user: &User,
        workspace: &str,
        chat: &str,
        input: &NewMessage,
        class: WorkClass<'_>,
    ) -> Result<Option<Job>> {
        let tx = self.workspace_tx(workspace).await?;
        let job = self
            .enqueue_chat_in(&tx, user, workspace, chat, input, class)
            .await?;
        tx.commit().await?;
        Ok(job)
    }
    pub(super) async fn communication_in(
        &self,
        tx: &DatabaseTransaction,
        user: &User,
        w: &str,
        chat: &str,
        input: &NewMessage,
    ) -> Result<Option<Job>> {
        ensure!(
            query(
                tx,
                "SELECT relay_communication_current($1,$2) AS allowed",
                vec![w.into(), chat.into()]
            )
            .await?
            .context("Missing communication scope")?
            .try_get::<bool>("", "allowed")?,
            "Communication audience or authority changed"
        );
        self.enqueue_chat_in(tx, user, w, chat, input, WorkClass::Communication)
            .await
    }
    // Scheduled/event intake shares the ordinary source/target/queue checks and
    // commits its occurrence receipt with the exact source message and job.
    pub(super) async fn background_in(
        &self,
        tx: &DatabaseTransaction,
        user: &User,
        workspace: &str,
        chat: &str,
        input: &NewMessage,
    ) -> Result<Option<Job>> {
        ensure!(
            self.pooled && input.agent.is_some(),
            "Background requests require pooled execution and an agent"
        );
        self.enqueue_chat_in(tx, user, workspace, chat, input, WorkClass::Background)
            .await
    }
    pub(super) async fn handoff_in(
        &self,
        tx: &DatabaseTransaction,
        parent: &Job,
        input: &NewMessage,
    ) -> Result<Option<Job>> {
        ensure!(
            self.pooled && input.agent.is_some(),
            "Handoffs require pooled agent execution"
        );
        super::control_actions::authorize(tx, parent, true).await?;
        let user = User {
            id: parent.requester.clone(),
            email: String::new(),
            display_name: None,
        };
        self.enqueue_chat_in(
            tx,
            &user,
            &parent.workspace,
            &parent.chat,
            input,
            WorkClass::Handoff(&parent.agent),
        )
        .await
    }
    pub(super) async fn private_in(
        &self,
        tx: &DatabaseTransaction,
        user: &User,
        w: &str,
        chat: &str,
        input: &NewMessage,
        agent: &str,
    ) -> Result<Option<Job>> {
        self.enqueue_chat_in(tx, user, w, chat, input, WorkClass::Private(agent))
            .await
    }
    async fn enqueue_chat_in(
        &self,
        tx: &DatabaseTransaction,
        user: &User,
        workspace: &str,
        chat: &str,
        input: &NewMessage,
        class: WorkClass<'_>,
    ) -> Result<Option<Job>> {
        ensure!(
            uuid::Uuid::parse_str(&input.id).is_ok(),
            "Invalid message ID"
        );
        ensure!(
            !input.content.trim().is_empty() && input.content.len() <= 12000,
            "Message is empty or too long"
        );
        access(tx, user, workspace, chat).await?;
        if class != WorkClass::Communication {
            super::communications::access::ordinary_chat(tx, workspace, chat).await?;
        }
        if class != WorkClass::Background {
            super::work_rooms::ordinary(tx, workspace, chat).await?;
        }
        let (author, kind) = match class {
            WorkClass::Handoff(sender) | WorkClass::Private(sender) => (sender, "agent"),
            _ => (user.id.as_str(), "user"),
        };
        if let Some(row) = query(
            tx,
            "SELECT chat,author,kind,content,target,thread,to_jsonb(m)->>'reply_to' AS reply_to,attachments FROM relay_chat_messages m WHERE id=$1",
            vec![input.id.clone().into()],
        )
        .await?
        {
            ensure!(
                row.try_get::<String>("", "chat")? == chat
                    && row.try_get::<String>("", "author")? == author
                    && row.try_get::<String>("", "kind")? == kind
                    && row.try_get::<String>("", "content")? == input.content
                    && row.try_get::<Option<String>>("", "target")? == input.agent
                    && row.try_get::<Option<String>>("", "thread")? == input.thread
                    && row.try_get::<Option<String>>("", "reply_to")? == input.reply_to
                    && row.try_get::<Value>("", "attachments")? == json!(input.attachments),
                "Message ID already used"
            );
            let existing = query(tx, "SELECT work_class FROM relay_chat_jobs WHERE request_message=$1", vec![input.id.clone().into()]).await?;
            let original = existing.map(|row| row.try_get::<String>("", "work_class")).transpose()?.unwrap_or_else(|| "interactive".into());
            ensure!(original == class.as_str(), "Request class cannot change on replay");
            return Ok(None);
        }
        super::channel_lifecycle::accepting(tx, workspace, chat).await?;
        for (kind, id) in
            crate::identity::chat::mentions::parse(&input.content, input.agent.as_deref())?
        {
            if kind == "user" {
                ensure!(query(tx,"SELECT m.user_id FROM relay_members m JOIN relay_chats c ON c.workspace=m.workspace WHERE m.workspace=$1 AND m.user_id=$2 AND c.id=$3 AND (c.visibility='public' OR EXISTS(SELECT 1 FROM relay_chat_people p WHERE p.workspace=$1 AND p.chat=$3 AND p.user_id=$2))",vec![workspace.into(),id.into(),chat.into()]).await?.is_some(),"Mentioned person is not in this conversation");
            }
            // Mentioned agents must match the explicit target, checked below against
            // current membership and requester authority in this transaction.
        }
        ensure!(input.attachments.len() <= 8, "Attach at most eight files");
        for id in &input.attachments {
            ensure!(query(tx,"SELECT id FROM relay_objects WHERE workspace=$1 AND chat=$2 AND id=$3 AND uploader=$4",vec![workspace.into(),chat.into(),id.clone().into(),user.id.clone().into()]).await?.is_some(),"Attachment is unavailable in this conversation");
        }
        let announcements: bool = query(
            tx,
            "SELECT announcements FROM relay_chats WHERE workspace=$1 AND id=$2",
            vec![workspace.into(), chat.into()],
        )
        .await?
        .context("Conversation missing")?
        .try_get("", "announcements")?;
        if announcements {
            manager(tx, workspace, &user.id).await?;
        }
        if let Some(thread) = &input.thread {
            ensure!(query(tx,"SELECT id FROM relay_chat_messages WHERE workspace=$1 AND chat=$2 AND id=$3 AND thread IS NULL",vec![workspace.into(),chat.into(),thread.clone().into()]).await?.is_some(),"Thread is not in this conversation");
        }
        if let Some(quoted) = &input.reply_to {
            // Same conversation and the same thread (a thread's root counts as inside it).
            ensure!(query(tx,"SELECT id FROM relay_chat_messages WHERE workspace=$1 AND chat=$2 AND id=$3 AND state<>'deleted' AND (thread IS NOT DISTINCT FROM $4 OR id=$4)",
                vec![workspace.into(),chat.into(),quoted.clone().into(),input.thread.clone().into()]).await?.is_some(),"Quoted message is not in this conversation");
        }
        if let Some(agent) = &input.agent {
            super::settings::pause::ensure_running(tx, workspace).await?;
            let pending:i64=query(tx,"SELECT count(*) AS total FROM relay_chat_messages WHERE workspace=$1 AND kind='agent' AND state='queued'",vec![workspace.into()]).await?.context("Missing count")?.try_get("","total")?;
            if class.background() {
                let count:i64=query(tx,"SELECT count(*) AS total FROM relay_chat_jobs j JOIN relay_chat_messages m ON m.id=j.reply WHERE j.workspace=$1 AND j.work_class='background' AND m.state='queued'",vec![workspace.into()]).await?.context("Missing background count")?.try_get("","total")?;
                if count >= 32 {
                    return Err(crate::identity::chat::model::BackgroundQueueFull.into());
                }
            }
            if pending >= 64 {
                return Err(crate::identity::chat::model::QueueFull.into());
            }
            ensure!(
                agent_access_in(tx, &user.id, workspace, agent, chat).await?,
                "Agent is not shared with you"
            );
            ensure!(
                super::settings::workspace::agent_model_allowed(tx, workspace, agent).await?,
                "This agent's model is no longer allowed in this workspace. Ask its owner to choose another model."
            );
            ensure!(query(tx,"SELECT agent FROM relay_chat_agents WHERE workspace=$1 AND chat=$2 AND agent=$3",vec![workspace.into(),chat.into(),agent.clone().into()]).await?.is_some(),"Agent is not in this conversation");
        }
        // reply_to (schema 40) is only named when used, so unquoted sends also work on older schemas.
        let (columns, values) = if input.reply_to.is_some() {
            (",reply_to", ",$11")
        } else {
            ("", "")
        };
        let mut args: Vec<sea_orm::Value> = vec![
            input.id.clone().into(),
            workspace.into(),
            chat.into(),
            author.into(),
            input.content.clone().into(),
            now().into(),
            input.agent.clone().into(),
            input.thread.clone().into(),
            json!(input.attachments).into(),
            kind.into(),
        ];
        if let Some(quoted) = &input.reply_to {
            args.push(quoted.clone().into());
        }
        execute(tx,&format!("INSERT INTO relay_chat_messages(id,workspace,chat,author,kind,content,state,at,target,thread,attachments{columns}) VALUES($1,$2,$3,$4,$10,$5,'sent',$6,$7,$8,$9{values})"),args).await?;
        let job = if let Some(agent) = &input.agent {
            let reply = uuid::Uuid::new_v4().to_string();
            let kind: String = query(
                tx,
                "SELECT kind FROM relay_chats WHERE workspace=$1 AND id=$2",
                vec![workspace.into(), chat.into()],
            )
            .await?
            .context("Conversation missing")?
            .try_get("", "kind")?;
            let managed = query(tx,"SELECT settings FROM relay_shared_agents WHERE workspace=$1 AND id=$2 AND revision>0",vec![workspace.into(),agent.clone().into()]).await?;
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
            execute(tx,"INSERT INTO relay_chat_messages(id,workspace,chat,author,kind,content,state,at,requester,thread) VALUES($1,$2,$3,$4,'agent','','queued',$5,$6,$7)",vec![reply.clone().into(),workspace.into(),chat.into(),agent.clone().into(),now().into(),user.id.clone().into(),input.thread.clone().into()]).await?;
            Some(Job {
                claim: None,
                reply,
                chat: chat.into(),
                workspace: workspace.into(),
                requester: user.id.clone(),
                agent: agent.clone(),
                prompt: super::conversation_context::prompt(&input.id, messages)?,
            })
        } else {
            None
        };
        if let Some(job) = &job {
            execute(
                tx,
                "INSERT INTO relay_chat_jobs(workspace,reply,payload,dispatch,run_seconds,work_class,request_message) VALUES($1,$2,$3,$4,$5,$6,$7)",
                vec![
                    workspace.into(),
                    job.reply.clone().into(),
                    serde_json::to_value(job)?.into(),
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
                super::chat::front_desk::enqueue_if_busy(tx, self.pooled, user, job, input).await?;
            }
        }
        if class == WorkClass::Interactive {
            super::communications::inbox::record_chat(
                tx,
                user,
                workspace,
                chat,
                input,
                job.as_ref(),
            )
            .await?;
        }
        Ok(job)
    }
}

//! Server-owned request classification; shared authorization and idempotent intake.
use super::{chat::access, *};
use crate::identity::chat::model::{Job, NewMessage};
use serde_json::json;

#[path = "several_agents.rs"]
pub(super) mod several_agents;
#[cfg(test)]
#[path = "several_agents_tests.rs"]
mod several_agents_tests;

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
    /// A person's message, with one job for each agent it asks (at most three).
    pub async fn chat_send_all(
        &self,
        user: &User,
        workspace: &str,
        chat: &str,
        input: &NewMessage,
    ) -> Result<Vec<Job>> {
        self.enqueue_chat(user, workspace, chat, input, WorkClass::Interactive)
            .await
    }
    /// [`Self::chat_send_all`] returning only the first job: for a message that
    /// asks at most one agent.
    pub async fn chat_send(
        &self,
        user: &User,
        workspace: &str,
        chat: &str,
        input: &NewMessage,
    ) -> Result<Option<Job>> {
        Ok(self
            .chat_send_all(user, workspace, chat, input)
            .await?
            .into_iter()
            .next())
    }
    pub(crate) async fn chat_background(
        &self,
        user: &User,
        workspace: &str,
        chat: &str,
        input: &NewMessage,
    ) -> Result<Option<Job>> {
        ensure!(
            self.pooled && !input.asked()?.is_empty(),
            "Background requests require pooled execution and an agent"
        );
        Ok(self
            .enqueue_chat(user, workspace, chat, input, WorkClass::Background)
            .await?
            .into_iter()
            .next())
    }
    async fn enqueue_chat(
        &self,
        user: &User,
        workspace: &str,
        chat: &str,
        input: &NewMessage,
        class: WorkClass<'_>,
    ) -> Result<Vec<Job>> {
        let tx = self.workspace_tx(workspace).await?;
        let jobs = self
            .enqueue_chat_in(&tx, user, workspace, chat, input, class)
            .await?;
        tx.commit().await?;
        Ok(jobs)
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
        self.one_job_in(tx, user, w, chat, input, WorkClass::Communication)
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
        self.one_job_in(tx, user, workspace, chat, input, WorkClass::Background)
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
        self.one_job_in(
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
        self.one_job_in(tx, user, w, chat, input, WorkClass::Private(agent))
            .await
    }
    /// Intake that asks one agent at most: events, schedules, handoffs, moved
    /// questions and email or WhatsApp messages.
    async fn one_job_in(
        &self,
        tx: &DatabaseTransaction,
        user: &User,
        w: &str,
        chat: &str,
        input: &NewMessage,
        class: WorkClass<'_>,
    ) -> Result<Option<Job>> {
        ensure!(input.agents.is_empty(), "This request asks one agent");
        Ok(self
            .enqueue_chat_in(tx, user, w, chat, input, class)
            .await?
            .into_iter()
            .next())
    }
    async fn enqueue_chat_in(
        &self,
        tx: &DatabaseTransaction,
        user: &User,
        workspace: &str,
        chat: &str,
        input: &NewMessage,
        class: WorkClass<'_>,
    ) -> Result<Vec<Job>> {
        ensure!(
            uuid::Uuid::parse_str(&input.id).is_ok(),
            "Invalid message ID"
        );
        ensure!(
            !input.content.trim().is_empty() && input.content.len() <= 12000,
            "Message is empty or too long"
        );
        let asked = input.asked()?;
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
        // The message keeps its first agent as `target`; the jobs hold every one.
        let target = asked.first().cloned();
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
                    && row.try_get::<Option<String>>("", "target")? == target
                    && row.try_get::<Option<String>>("", "thread")? == input.thread
                    && row.try_get::<Option<String>>("", "reply_to")? == input.reply_to
                    && row.try_get::<Value>("", "attachments")? == json!(input.attachments),
                "Message ID already used"
            );
            several_agents::replayed(tx, workspace, &input.id, &asked, class.as_str()).await?;
            return Ok(vec![]);
        }
        super::channel_lifecycle::accepting(tx, workspace, chat).await?;
        let mentions = crate::identity::chat::mentions::parse(&input.content)?;
        // A handoff task may name other agents as plain text; only its target is
        // asked. Otherwise every mentioned agent must be asked, each checked
        // below against current membership and requester authority.
        if !matches!(class, WorkClass::Handoff(_)) {
            crate::identity::chat::mentions::ensure_target(&mentions, &asked)?;
        }
        mentioned_people_visible(tx, workspace, chat, &mentions).await?;
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
        // Every asked agent takes the message, or none does (several_agents.rs).
        several_agents::admit(tx, user, workspace, chat, &asked, class.background()).await?;
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
            target.into(),
            input.thread.clone().into(),
            json!(input.attachments).into(),
            kind.into(),
        ];
        if let Some(quoted) = &input.reply_to {
            args.push(quoted.clone().into());
        }
        execute(tx,&format!("INSERT INTO relay_chat_messages(id,workspace,chat,author,kind,content,state,at,target,thread,attachments{columns}) VALUES($1,$2,$3,$4,$10,$5,'sent',$6,$7,$8,$9{values})"),args).await?;
        let mut jobs = Vec::with_capacity(asked.len());
        for agent in &asked {
            jobs.push(
                self.queue_reply(tx, user, workspace, chat, input, agent, class)
                    .await?,
            );
        }
        if class == WorkClass::Interactive {
            super::communications::inbox::record_chat(
                tx,
                user,
                workspace,
                chat,
                input,
                jobs.first(),
            )
            .await?;
        }
        Ok(jobs)
    }
    /// The agent's own queued reply and job for an accepted message, with its
    /// delivered tick, and a front-desk answer when it is busy.
    #[allow(clippy::too_many_arguments)]
    async fn queue_reply(
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
        execute(tx,"INSERT INTO relay_chat_messages(id,workspace,chat,author,kind,content,state,at,requester,thread) VALUES($1,$2,$3,$4,'agent','','queued',$5,$6,$7)",vec![reply.clone().into(),workspace.into(),chat.into(),agent.into(),now().into(),user.id.clone().into(),input.thread.clone().into()]).await?;
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

/// Every mentioned person can read this conversation. Sends and edits share it.
pub(super) async fn mentioned_people_visible(
    tx: &DatabaseTransaction,
    workspace: &str,
    chat: &str,
    mentions: &[(String, String)],
) -> Result<()> {
    for (kind, id) in mentions {
        if kind == "user" {
            ensure!(query(tx,"SELECT m.user_id FROM relay_members m JOIN relay_chats c ON c.workspace=m.workspace WHERE m.workspace=$1 AND m.user_id=$2 AND c.id=$3 AND (c.visibility='public' OR EXISTS(SELECT 1 FROM relay_chat_people p WHERE p.workspace=$1 AND p.chat=$3 AND p.user_id=$2))",vec![workspace.into(),id.clone().into(),chat.into()]).await?.is_some(),"Mentioned person is not in this conversation");
        }
    }
    Ok(())
}

//! Server-owned request classification; shared authorization and idempotent intake.
use super::{chat::access, message_actions::agent_reactions, *};
use crate::identity::chat::model::{Job, NewMessage};
use serde_json::json;

#[path = "message_forwarding.rs"]
pub(super) mod message_forwarding;
#[path = "reply_queue.rs"]
mod reply_queue;
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
    /// An emoji-only message gets each agent's reaction instead, and no jobs.
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
            .enqueue_chat_in(&tx, user, workspace, chat, input, class, None)
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
    // Task and event intake shares the ordinary source/target/queue checks and
    // commits its receipt with the exact source message and job.
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
    /// Intake that asks one agent at most: events, task runs, handoffs, moved
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
            .enqueue_chat_in(tx, user, w, chat, input, class, None)
            .await?
            .into_iter()
            .next())
    }
    /// A message a member's assistant wrote for them in a thread (schema 128):
    /// an ordinary message from the member, asking the agent it goes to.
    pub(super) async fn as_owner_in(
        &self,
        tx: &DatabaseTransaction,
        user: &User,
        w: &str,
        chat: &str,
        input: &NewMessage,
    ) -> Result<Vec<Job>> {
        self.enqueue_chat_in(tx, user, w, chat, input, WorkClass::Interactive, None)
            .await
    }
    /// A person's message carrying a copy of another message they forward
    /// (schema 125): an ordinary message, asking the agent it goes to.
    pub(super) async fn forward_in(
        &self,
        tx: &DatabaseTransaction,
        user: &User,
        w: &str,
        chat: &str,
        input: &NewMessage,
        forwarded: &Value,
    ) -> Result<Vec<Job>> {
        self.enqueue_chat_in(
            tx,
            user,
            w,
            chat,
            input,
            WorkClass::Interactive,
            Some(forwarded),
        )
        .await
    }
    #[allow(clippy::too_many_arguments)]
    async fn enqueue_chat_in(
        &self,
        tx: &DatabaseTransaction,
        user: &User,
        workspace: &str,
        chat: &str,
        input: &NewMessage,
        class: WorkClass<'_>,
        forwarded: Option<&Value>,
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
        // A person answering an agent that asked them answers its waiting task.
        let asked = if class == WorkClass::Interactive {
            super::agent_teamwork::answering(tx, workspace, chat, &user.id, input, asked).await?
        } else {
            asked
        };
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
        // A person's emoji-only message to agents gets each one's reaction
        // instead of a model run (D246, agent_reactions.rs).
        let reaction =
            if class == WorkClass::Interactive && !asked.is_empty() && input.attachments.is_empty()
            {
                agent_reactions::reaction(&input.content)
            } else {
                None
            };
        if let Some(row) = query(
            tx,
            "SELECT chat,author,kind,content,target,thread,to_jsonb(m)->>'reply_to' AS reply_to,attachments,to_jsonb(m)->'forwarded' AS forwarded FROM relay_chat_messages m WHERE id=$1",
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
                    && row.try_get::<Value>("", "attachments")? == json!(input.attachments)
                    && row.try_get::<Option<Value>>("", "forwarded")?.filter(|v| !v.is_null()).as_ref() == forwarded,
                "Message ID already used"
            );
            several_agents::replayed(
                tx,
                workspace,
                &input.id,
                &asked,
                class.as_str(),
                reaction.is_some(),
            )
            .await?;
            return Ok(vec![]);
        }
        super::channel_lifecycle::accepting(tx, workspace, chat).await?;
        let mentions = crate::identity::chat::mentions::parse(&input.content)?;
        // A handoff task may name other agents as plain text; only its target is
        // asked. Otherwise every mentioned agent must be asked, each checked
        // below against current membership and requester authority.
        if !matches!(class, WorkClass::Handoff(_)) {
            // What a person asks their own assistant is private. A message that
            // mentions it without asking it would post those words for everyone.
            for (kind, id) in &mentions {
                ensure!(
                    kind != "agent"
                        || asked.contains(id)
                        || !super::assistants::works_for(tx, workspace, id, &user.id).await?,
                    "This asks your assistant, so it was not posted. Ask it from the message box, or remove its mention."
                );
            }
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
        let asking = match reaction {
            Some(_) => several_agents::Asking::Reaction,
            None => several_agents::Asking::Work {
                background: class.background(),
            },
        };
        several_agents::admit(tx, user, workspace, chat, &asked, asking).await?;
        // reply_to (schema 40) is only named when used, so unquoted sends also work on older schemas.
        let (mut columns, mut values) = (String::new(), String::new());
        if input.reply_to.is_some() {
            columns.push_str(",reply_to");
            values.push_str(",$11");
        }
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
        // forwarded (schema 125) is only named when used, like reply_to.
        if let Some(copy) = forwarded {
            args.push(copy.clone().into());
            columns.push_str(",forwarded");
            values.push_str(&format!(",${}", args.len()));
        }
        execute(tx,&format!("INSERT INTO relay_chat_messages(id,workspace,chat,author,kind,content,state,at,target,thread,attachments{columns}) VALUES($1,$2,$3,$4,$10,$5,'sent',$6,$7,$8,$9{values})"),args).await?;
        let mut jobs = Vec::with_capacity(asked.len());
        for agent in &asked {
            if let Some(emoji) = reaction {
                agent_reactions::acknowledge(tx, workspace, chat, &input.id, agent, emoji).await?;
                continue;
            }
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

//! One message may ask up to three agents (the owner's decision of 2026-11-07,
//! reversing the one-agent rule of 2026-10-21). Each asked agent answers
//! separately: its own reply and job, with its own permissions, budget, model
//! and receipts, and one agent failing or being stopped never blocks the others.
//!
//! Intake is all or nothing. Every agent passes every check first, and if one
//! can't take the message, nothing is saved and the person is told which agent
//! and why. A partial send would leave a message that @mentions an agent who was
//! never asked, which reads as if it was; and since an outbox retry replays the
//! same ID, a partial send would also have to remember which agents were left
//! out. All or nothing keeps a replay simple: the message and one job per asked
//! agent exist together or not at all, so a replay finds exactly that set.
//!
//! Schema 99 lets several jobs share a request message: unique per message and
//! agent (`relay_jobs_request_agent`) instead of per message.
use super::super::{
    chat::agent_access_in,
    model_budgets::budget_reached,
    settings::{pause::ensure_running, workspace::agent_model_allowed},
    *,
};
use crate::identity::chat::model::{BackgroundQueueFull, QueueFull, refused};
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};
use std::collections::BTreeSet;

pub(crate) struct SeveralAgentsPerMessage;
impl MigrationName for SeveralAgentsPerMessage {
    fn name(&self) -> &str {
        "m20261107_000099_several_agents_per_message"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for SeveralAgentsPerMessage {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("several_agents.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Several agents per message requires a forward migration".into(),
        ))
    }
}

/// What the asked agents take a message for.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Asking {
    /// Work for each agent, in the interactive or the background queue.
    Work { background: bool },
    /// An emoji-only message each agent only reacts to (agent_reactions.rs). It
    /// takes no queue place, model call or budget, so none of those is checked.
    Reaction,
}

/// Every asked agent may take this message, or none is asked: the queue has
/// room for all of them, and each passes the same checks a single agent always
/// has. One refused agent refuses the message, naming each agent and its reason.
pub(super) async fn admit(
    tx: &DatabaseTransaction,
    user: &User,
    w: &str,
    chat: &str,
    asked: &[String],
    asking: Asking,
) -> Result<()> {
    if asked.is_empty() {
        return Ok(());
    }
    ensure_running(tx, w).await?;
    if let Asking::Work { background } = asking {
        queue_room(tx, w, asked.len() as i64, background).await?;
    }
    let mut refusals = Vec::new();
    for agent in asked {
        if let Some(reason) = refusal(tx, user, w, chat, agent, asking).await? {
            refusals.push((agent, reason));
        }
    }
    match refusals.as_slice() {
        [] => Ok(()),
        // Asking one agent keeps the reasons people have always seen.
        [(_, reason)] if asked.len() == 1 => anyhow::bail!(reason.clone()),
        _ => {
            let mut named = Vec::new();
            for (agent, reason) in refusals {
                named.push((name(tx, w, chat, agent).await?, reason));
            }
            anyhow::bail!(refused(asked.len(), &named))
        }
    }
}

/// The queue has room for `more` replies.
async fn queue_room(tx: &DatabaseTransaction, w: &str, more: i64, background: bool) -> Result<()> {
    let pending: i64 = query(tx, "SELECT count(*) AS total FROM relay_chat_messages WHERE workspace=$1 AND kind='agent' AND state='queued'", vec![w.into()])
        .await?
        .context("Missing count")?
        .try_get("", "total")?;
    if background {
        let count: i64 = query(tx, "SELECT count(*) AS total FROM relay_chat_jobs j JOIN relay_chat_messages m ON m.id=j.reply WHERE j.workspace=$1 AND j.work_class='background' AND m.state='queued'", vec![w.into()])
            .await?
            .context("Missing background count")?
            .try_get("", "total")?;
        if count + more > 32 {
            return Err(BackgroundQueueFull.into());
        }
    }
    if pending + more > 64 {
        return Err(QueueFull.into());
    }
    Ok(())
}

/// Why `agent` can't take this person's message here, if it can't.
async fn refusal(
    tx: &DatabaseTransaction,
    user: &User,
    w: &str,
    chat: &str,
    agent: &str,
    asking: Asking,
) -> Result<Option<String>> {
    let here = query(
        tx,
        "SELECT COALESCE((a.settings->>'paused')::BOOLEAN,FALSE) AS paused FROM relay_chat_agents x
         LEFT JOIN relay_shared_agents a ON a.workspace=x.workspace AND a.id=x.agent
         WHERE x.workspace=$1 AND x.chat=$2 AND x.agent=$3",
        vec![w.into(), chat.into(), agent.into()],
    )
    .await?;
    if !agent_access_in(tx, &user.id, w, agent, chat).await? {
        // Only an agent in this conversation, which its people already see, is
        // said to be paused.
        let paused = here
            .map(|row| row.try_get::<bool>("", "paused"))
            .transpose()?
            .unwrap_or(false);
        return Ok(Some(if paused {
            "This agent is paused. Its manager or an admin can resume it.".into()
        } else {
            "Agent is not shared with you".into()
        }));
    }
    if asking != Asking::Reaction && !agent_model_allowed(tx, w, agent).await? {
        return Ok(Some("This agent's model is no longer allowed in this workspace. Ask its owner to choose another model.".into()));
    }
    if asking != Asking::Reaction
        && let Some(reason) = budget_reached(tx, w, agent).await?
    {
        return Ok(Some(reason));
    }
    if here.is_none() {
        return Ok(Some("Agent is not in this conversation".into()));
    }
    Ok(None)
}

/// The agent's name for a refusal: shown only for an agent of this
/// conversation, whose name its people already see.
async fn name(tx: &DatabaseTransaction, w: &str, chat: &str, agent: &str) -> Result<String> {
    let row = query(
        tx,
        "SELECT a.name FROM relay_chat_agents x JOIN relay_shared_agents a ON a.workspace=x.workspace AND a.id=x.agent
         WHERE x.workspace=$1 AND x.chat=$2 AND x.agent=$3",
        vec![w.into(), chat.into(), agent.into()],
    )
    .await?;
    Ok(row
        .map(|row| row.try_get::<String>("", "name"))
        .transpose()?
        .unwrap_or_else(|| "An agent you asked".into()))
}

/// A replayed message ID asks exactly the agents it asked the first time, in
/// the same request class; an exact replay starts nothing new. `reaction`: the
/// message is emoji-only, which agents react to instead of answering.
pub(super) async fn replayed(
    tx: &DatabaseTransaction,
    w: &str,
    id: &str,
    asked: &[String],
    class: &str,
    reaction: bool,
) -> Result<()> {
    let jobs = rows(
        tx,
        "SELECT payload->>'agent' AS agent,work_class FROM relay_chat_jobs WHERE workspace=$1 AND request_message=$2",
        vec![w.into(), id.into()],
    )
    .await?;
    // A message without jobs was an ordinary interactive send.
    ensure!(
        !jobs.is_empty() || class == "interactive",
        "Request class cannot change on replay"
    );
    let mut linked = BTreeSet::new();
    for job in &jobs {
        linked.insert(job.try_get::<String>("", "agent")?);
        ensure!(
            job.try_get::<String>("", "work_class")? == class,
            "Request class cannot change on replay"
        );
    }
    // Each agent reacted instead of taking a job (schema 101). The same message
    // saved before then has jobs, found above.
    if reaction && jobs.is_empty() {
        linked = super::super::message_actions::agent_reactions::acknowledged(tx, w, id).await?;
    }
    // Requests saved before schema 16 have no job linked to their message.
    let wanted: BTreeSet<String> = asked.iter().cloned().collect();
    ensure!(
        linked == wanted || (linked.is_empty() && asked.len() <= 1),
        "Message ID already used"
    );
    Ok(())
}

//! Agents a person invites into one thread of their private chat (schema 125).
//!
//! In your direct chat with your own only-me agent, mentioning another agent
//! asks your agent to work with it. Cuckoo answers in a thread under your
//! message, and the mentioned agent becomes a guest of that thread:
//!
//! - It is never a member of the chat. It reads that thread and nothing else of
//!   it, and works there only when your agent hands it a task.
//! - Only agents you may talk to, that take work from other agents, can be
//!   invited, at most two per thread (the hand-off limit of two helpers).
//! - Your own agent keeps reading the whole chat, as before.
use super::*;
use crate::identity::chat::model::Job;

#[cfg(test)]
#[path = "thread_guests_tests.rs"]
mod tests;

/// The most agents one thread may have as guests.
const MOST: i64 = 2;

/// Whether schema 125 is applied. Upgrade tests run this code on older schemas.
pub(super) async fn ready(db: &impl ConnectionTrait) -> Result<bool> {
    Ok(query(
        db,
        "SELECT to_regclass('relay_thread_guests') IS NOT NULL AS ready",
        vec![],
    )
    .await?
    .context("Missing schema check")?
    .try_get("", "ready")?)
}

/// Your own only-me agent, when `chat` is your direct chat with it.
pub(super) async fn personal_agent(
    db: &impl ConnectionTrait,
    person: &str,
    w: &str,
    chat: &str,
) -> Result<Option<String>> {
    if !ready(db).await? {
        return Ok(None);
    }
    let row = query(
        db,
        "SELECT x.agent FROM relay_chats c
         JOIN relay_chat_agents x ON x.workspace=c.workspace AND x.chat=c.id
         JOIN relay_shared_agents a ON a.workspace=x.workspace AND a.id=x.agent
         WHERE c.workspace=$1 AND c.id=$2 AND c.kind='direct' AND NOT c.work
           AND a.audience='only_me' AND a.revision>0 AND relay_agent_owns($1,a.id,$3)
           AND (SELECT count(*) FROM relay_chat_agents y WHERE y.workspace=c.workspace AND y.chat=c.id)=1",
        vec![w.into(), chat.into(), person.into()],
    )
    .await?;
    Ok(row.map(|row| row.try_get("", "agent")).transpose()?)
}

/// The other agents a message mentions in your chat with your own agent,
/// checked: each is one you may talk to, a managed shared agent, and one that
/// takes work from other agents.
pub(super) async fn invited(
    tx: &DatabaseTransaction,
    person: &str,
    w: &str,
    own: &str,
    mentions: &[(String, String)],
) -> Result<Vec<String>> {
    let mut agents: Vec<String> = vec![];
    for (kind, id) in mentions {
        if kind == "agent" && id != own && !agents.contains(id) {
            agents.push(id.clone());
        }
    }
    for agent in &agents {
        ensure!(
            super::chat::agent_access(tx, person, w, agent).await?,
            "Agent is not shared with you"
        );
        let profile = super::agent_profiles::stored(tx, w, agent).await?;
        ensure!(
            profile["audience"] != "only_me" && profile["revision"].as_i64().is_some_and(|r| r > 0),
            "Only shared agents can work with your agent"
        );
        ensure!(
            profile["settings"]["helper"] != false,
            "{} doesn't take work from other agents",
            profile["name"].as_str().unwrap_or("This agent")
        );
    }
    ensure!(
        agents.len() as i64 <= MOST,
        "Invite at most two agents to work with your agent"
    );
    Ok(agents)
}

/// Make `agents` guests of `thread`. Inviting one already there changes nothing.
pub(super) async fn invite(
    tx: &DatabaseTransaction,
    person: &str,
    w: &str,
    chat: &str,
    thread: &str,
    agents: &[String],
) -> Result<()> {
    for agent in agents {
        execute(
            tx,
            "INSERT INTO relay_thread_guests(workspace,chat,thread,agent,invited_by) VALUES($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING",
            vec![w.into(), chat.into(), thread.into(), agent.clone().into(), person.into()],
        )
        .await?;
    }
    let count: i64 = query(
        tx,
        "SELECT count(*) AS count FROM relay_thread_guests WHERE workspace=$1 AND chat=$2 AND thread=$3",
        vec![w.into(), chat.into(), thread.into()],
    )
    .await?
    .context("Missing guest count")?
    .try_get("", "count")?;
    ensure!(
        count <= MOST,
        "A thread can have at most two agents working with your agent"
    );
    Ok(())
}

/// Whether `agent` is a guest of `thread`.
pub(super) async fn is_guest(
    db: &impl ConnectionTrait,
    w: &str,
    chat: &str,
    thread: Option<&str>,
    agent: &str,
) -> Result<bool> {
    let Some(thread) = thread else {
        return Ok(false);
    };
    if !ready(db).await? {
        return Ok(false);
    }
    Ok(query(
        db,
        "SELECT 1 AS found FROM relay_thread_guests WHERE workspace=$1 AND chat=$2 AND thread=$3 AND agent=$4",
        vec![w.into(), chat.into(), thread.into(), agent.into()],
    )
    .await?
    .is_some())
}

/// Whether `agent` takes part in `chat` only as a guest of some of its threads.
pub(super) async fn guest_only(
    db: &impl ConnectionTrait,
    w: &str,
    chat: &str,
    agent: &str,
) -> Result<bool> {
    if !ready(db).await? {
        return Ok(false);
    }
    Ok(query(
        db,
        "SELECT 1 AS found WHERE EXISTS(SELECT 1 FROM relay_thread_guests WHERE workspace=$1 AND chat=$2 AND agent=$3)
           AND NOT EXISTS(SELECT 1 FROM relay_chat_agents WHERE workspace=$1 AND chat=$2 AND agent=$3)",
        vec![w.into(), chat.into(), agent.into()],
    )
    .await?
    .is_some())
}

/// A guest reads its own thread and nothing else of the chat. Every read of a
/// conversation for an agent passes this check (conversation_context.rs).
pub(super) async fn confine(
    db: &impl ConnectionTrait,
    w: &str,
    chat: &str,
    agent: &str,
    thread: Option<&str>,
    main_only: bool,
) -> Result<()> {
    if guest_only(db, w, chat, agent).await? {
        ensure!(
            !main_only && is_guest(db, w, chat, thread, agent).await?,
            "Agent is not in this conversation"
        );
    }
    Ok(())
}

/// The thread `job`'s reply is in, when it is a thread with guests in a direct
/// chat: where a person's own agent and the agents they invited work together.
pub(super) async fn job_thread(db: &impl ConnectionTrait, job: &Job) -> Result<Option<String>> {
    if !ready(db).await? {
        return Ok(None);
    }
    let row = query(
        db,
        "SELECT m.thread FROM relay_chat_messages m JOIN relay_chats c ON c.workspace=m.workspace AND c.id=m.chat
         WHERE m.workspace=$1 AND m.id=$2 AND c.kind='direct'
           AND EXISTS(SELECT 1 FROM relay_thread_guests g WHERE g.workspace=m.workspace AND g.chat=m.chat AND g.thread=m.thread)",
        vec![job.workspace.clone().into(), job.reply.clone().into()],
    )
    .await?;
    Ok(row
        .map(|row| row.try_get::<Option<String>>("", "thread"))
        .transpose()?
        .flatten())
}

/// Whether `agent` takes part in the conversation of its task `reply`: a member
/// of the chat, or a guest of the thread the reply is in.
pub(super) async fn takes_part(
    db: &impl ConnectionTrait,
    w: &str,
    chat: &str,
    agent: &str,
    reply: &str,
) -> Result<bool> {
    let guests = if ready(db).await? {
        " OR EXISTS(SELECT 1 FROM relay_chat_messages m JOIN relay_thread_guests g ON g.workspace=m.workspace AND g.chat=m.chat AND g.thread=m.thread
             WHERE m.workspace=$1 AND m.chat=$2 AND m.id=$4 AND g.agent=$3)"
    } else {
        ""
    };
    Ok(query(
        db,
        &format!("SELECT 1 AS found WHERE EXISTS(SELECT 1 FROM relay_chat_agents WHERE workspace=$1 AND chat=$2 AND agent=$3){guests}"),
        vec![w.into(), chat.into(), agent.into(), reply.into()],
    )
    .await?
    .is_some())
}

/// Whether `thread` of `chat` has guests: an agent's answer there stays in it.
pub(super) async fn has_guests(
    db: &impl ConnectionTrait,
    w: &str,
    chat: &str,
    thread: &str,
) -> Result<bool> {
    if !ready(db).await? {
        return Ok(false);
    }
    Ok(query(
        db,
        "SELECT 1 AS found FROM relay_thread_guests WHERE workspace=$1 AND chat=$2 AND thread=$3 LIMIT 1",
        vec![w.into(), chat.into(), thread.into()],
    )
    .await?
    .is_some())
}

pub(crate) use migration::ForwardingAndGuests;
mod migration {
    use sea_orm_migration::prelude::*;
    pub(crate) struct ForwardingAndGuests;
    impl MigrationName for ForwardingAndGuests {
        fn name(&self) -> &str {
            "m20261116_000125_forwarding_and_guests"
        }
    }
    #[async_trait::async_trait]
    impl MigrationTrait for ForwardingAndGuests {
        async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
            m.get_connection()
                .execute_unprepared(include_str!("forwarding_and_guests.sql"))
                .await?;
            Ok(())
        }
        async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
            Err(DbErr::Custom(
                "Forwarding and thread guests require a forward migration".into(),
            ))
        }
    }
}

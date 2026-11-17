//! Every member's own assistant (schema 127).
//!
//! An assistant works for one person. It is an ordinary managed agent that only
//! its owner can talk to, with these fixed rules:
//!
//! - one per member and workspace, made the first time they open Chat, named
//!   Ayati until they rename it;
//! - it stays private to its owner: its audience can't change, it has no other
//!   owners, joins no channel and has no email address;
//! - its owner and admins see its work in Activity, like any agent's.
use super::*;
use crate::identity::{
    agents::config::{SaveAgent, Settings},
    chat::model::NewChat,
};

#[path = "assistant_settings.rs"]
pub(crate) mod settings;
#[cfg(test)]
#[path = "assistants_tests.rs"]
mod tests;
#[path = "assistant_threads.rs"]
pub(crate) mod threads;

/// What a new assistant is called until its owner renames it.
pub(crate) const NAME: &str = "Ayati";

/// Whether schema 127 is applied. Upgrade tests run this code on older schemas.
pub(super) async fn ready(db: &impl ConnectionTrait) -> Result<bool> {
    Ok(query(
        db,
        "SELECT to_regclass('relay_assistants') IS NOT NULL AS ready",
        vec![],
    )
    .await?
    .context("Missing schema check")?
    .try_get("", "ready")?)
}

/// Whether `agent` is someone's assistant.
pub(super) async fn is_assistant(db: &impl ConnectionTrait, w: &str, agent: &str) -> Result<bool> {
    if !ready(db).await? {
        return Ok(false);
    }
    Ok(query(
        db,
        "SELECT 1 AS found FROM relay_assistants WHERE workspace=$1 AND agent=$2",
        vec![w.into(), agent.into()],
    )
    .await?
    .is_some())
}

/// SQL for "agent `a` is an assistant", or `false` before schema 127.
pub(super) async fn flag(db: &impl ConnectionTrait) -> Result<&'static str> {
    Ok(if ready(db).await? {
        "EXISTS(SELECT 1 FROM relay_assistants s WHERE s.workspace=a.workspace AND s.agent=a.id)"
    } else {
        "false"
    })
}

/// SQL for "message `m` is in an assistant's own room": its chat with its owner.
/// There its owner is the only reader, so what its tools and events told it
/// stays in its memory (conversation_context.rs).
pub(super) async fn own_room(db: &impl ConnectionTrait, m: &str) -> Result<String> {
    Ok(if ready(db).await? {
        format!(
            "EXISTS(SELECT 1 FROM relay_assistants s JOIN relay_chat_agents x ON x.workspace=s.workspace AND x.agent=s.agent WHERE s.workspace={m}.workspace AND x.chat={m}.chat)"
        )
    } else {
        "FALSE".into()
    })
}

/// Where an assistant's schedule or trigger runs: its owner's chat with it, in
/// the automation's own thread, opened under one line that names it. `None`
/// for any other agent, which runs in a work room.
pub(super) async fn run_place(
    tx: &DatabaseTransaction,
    w: &str,
    agent: &str,
    person: &str,
    automation: &str,
    title: &str,
) -> Result<Option<(String, String)>> {
    if !is_assistant(tx, w, agent).await? {
        return Ok(None);
    }
    if let Some(row) = query(
        tx,
        "SELECT chat,thread FROM relay_assistant_threads WHERE workspace=$1 AND agent=$2 AND automation=$3",
        vec![w.into(), agent.into(), automation.into()],
    )
    .await?
    {
        return Ok(Some((row.try_get("", "chat")?, row.try_get("", "thread")?)));
    }
    let Some(row) = query(
        tx,
        "SELECT c.id FROM relay_chats c
         JOIN relay_chat_agents x ON x.workspace=c.workspace AND x.chat=c.id AND x.agent=$2
         JOIN relay_chat_people p ON p.workspace=c.workspace AND p.chat=c.id AND p.user_id=$3
         JOIN relay_assistants s ON s.workspace=c.workspace AND s.agent=x.agent AND s.user_id=p.user_id
         WHERE c.workspace=$1 AND c.kind='direct' AND NOT c.work",
        vec![w.into(), agent.into(), person.into()],
    )
    .await?
    else {
        return Ok(None);
    };
    let chat: String = row.try_get("", "id")?;
    let thread = uuid::Uuid::new_v4().to_string();
    execute(
        tx,
        "INSERT INTO relay_chat_messages(id,workspace,chat,author,kind,content,state,at,requester) VALUES($1,$2,$3,$4,'agent',$5,'complete',$6,$7)",
        vec![
            thread.clone().into(),
            w.into(),
            chat.clone().into(),
            agent.into(),
            title.chars().take(200).collect::<String>().into(),
            now().into(),
            person.into(),
        ],
    )
    .await?;
    execute(
        tx,
        "INSERT INTO relay_assistant_threads(workspace,agent,automation,chat,thread) VALUES($1,$2,$3,$4,$5)",
        vec![w.into(), agent.into(), automation.into(), chat.clone().into(), thread.clone().into()],
    )
    .await?;
    Ok(Some((chat, thread)))
}

/// An assistant keeps its fixed rules when its profile is saved.
pub(super) async fn keep_private(
    db: &impl ConnectionTrait,
    w: &str,
    input: &SaveAgent,
) -> Result<()> {
    if is_assistant(db, w, &input.id).await? {
        ensure!(
            input.audience == "only_me",
            "Your assistant stays private to you"
        );
    }
    Ok(())
}

impl PgStore {
    /// The member's assistant, once it exists.
    pub async fn assistant_of(&self, user: &User, w: &str) -> Result<Option<String>> {
        if !ready(&self.db).await? {
            return Ok(None);
        }
        let row = query(
            &self.db,
            "SELECT s.agent FROM relay_assistants s
             JOIN relay_shared_agents a ON a.workspace=s.workspace AND a.id=s.agent
             WHERE s.workspace=$1 AND s.user_id=$2",
            vec![w.into(), user.id.clone().into()],
        )
        .await?;
        Ok(row.map(|row| row.try_get("", "agent")).transpose()?)
    }

    /// Whether this task is an assistant working for its own person.
    pub(crate) async fn works_for_its_person(
        &self,
        w: &str,
        agent: &str,
        person: &str,
    ) -> Result<bool> {
        Ok(ready(&self.db).await?
            && query(
                &self.db,
                "SELECT 1 AS found FROM relay_assistants WHERE workspace=$1 AND agent=$2 AND user_id=$3",
                vec![w.into(), agent.into(), person.into()],
            )
            .await?
            .is_some())
    }

    /// Whether a new agent with this id would be created by saving it.
    pub async fn agent_is_new(&self, w: &str, id: &str) -> Result<bool> {
        Ok(query(
            &self.db,
            "SELECT 1 AS found FROM relay_shared_agents WHERE workspace=$1 AND id=$2",
            vec![w.into(), id.into()],
        )
        .await?
        .is_none())
    }

    /// The member's assistant and their chat with it, made if they have none.
    /// Safe to call again and from two requests at once: the member's row
    /// decides the agent id before the agent is saved.
    pub async fn ensure_assistant(
        &self,
        user: &User,
        w: &str,
        model: &str,
        provider: &str,
    ) -> Result<String> {
        if let Some(agent) = self.assistant_of(user, w).await? {
            return Ok(agent);
        }
        let tx = self.workspace_tx(w).await?;
        role(&tx, w, &user.id).await?;
        execute(
            &tx,
            "INSERT INTO relay_assistants(workspace,user_id,agent) VALUES($1,$2,$3) ON CONFLICT(workspace,user_id) DO NOTHING",
            vec![
                w.into(),
                user.id.clone().into(),
                uuid::Uuid::new_v4().to_string().into(),
            ],
        )
        .await?;
        let agent: String = query(
            &tx,
            "SELECT agent FROM relay_assistants WHERE workspace=$1 AND user_id=$2",
            vec![w.into(), user.id.clone().into()],
        )
        .await?
        .context("Assistant missing")?
        .try_get("", "agent")?;
        tx.commit().await?;
        let mut settings = Settings {
            provider: provider.into(),
            ..Default::default()
        };
        // Its own computer and browser, where this Cuckoo has computers.
        settings.computer.on = self.computers.is_some();
        self.save_agent_profile(
            user,
            w,
            &SaveAgent {
                id: agent.clone(),
                name: NAME.into(),
                role: "Your assistant".into(),
                model: model.into(),
                audience: "only_me".into(),
                people: None,
                revision: 0,
                settings,
            },
        )
        .await?;
        self.chat_create(
            user,
            w,
            &NewChat {
                id: uuid::Uuid::new_v4().to_string(),
                name: NAME.into(),
                users: vec![],
                agents: vec![agent.clone()],
                kind: "direct".into(),
                description: String::new(),
            },
        )
        .await?;
        Ok(agent)
    }
}

pub(crate) use migration::Assistants;
mod migration {
    use sea_orm_migration::prelude::*;
    pub(crate) struct Assistants;
    impl MigrationName for Assistants {
        fn name(&self) -> &str {
            "m20261117_000127_assistants"
        }
    }
    #[async_trait::async_trait]
    impl MigrationTrait for Assistants {
        async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
            m.get_connection()
                .execute_unprepared(include_str!("assistants.sql"))
                .await?;
            Ok(())
        }
        async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
            Err(DbErr::Custom(
                "Assistants require a forward migration".into(),
            ))
        }
    }
}

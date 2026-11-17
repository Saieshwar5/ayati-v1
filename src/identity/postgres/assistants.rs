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

#[cfg(test)]
#[path = "assistants_tests.rs"]
mod tests;

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

//! The agent a member is still setting up in New agent (schema 139).
//!
//! What they chose and wrote so far is kept, one draft per member and
//! workspace, so a reload or another device does not lose it. It is theirs
//! alone and it is not an agent: saving it creates nothing, and the agent is
//! made only by the ordinary save when they press Create agent.
use super::*;
use crate::identity::agents::config::INSTRUCTIONS_LIMIT;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[cfg(test)]
#[path = "agent_drafts_tests.rs"]
mod tests;

/// A skill written in New agent and not saved yet.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WrittenSkill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub steps: String,
}
/// A library skill picked for the new agent.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PickedSkill {
    pub id: String,
    pub name: String,
    pub description: String,
}
/// One message of the setup chat.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Said {
    pub role: String,
    pub text: String,
}
/// New agent as far as the member got: the three steps' fields.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentDraft {
    /// The id the agent will have once it is created.
    pub id: String,
    pub name: String,
    pub role: String,
    pub avatar: String,
    pub instructions: String,
    pub audience: String,
    #[serde(default)]
    pub people: Vec<String>,
    pub step: u8,
    #[serde(default)]
    pub skills: Vec<WrittenSkill>,
    #[serde(default)]
    pub library: Vec<PickedSkill>,
    #[serde(default)]
    pub messages: Vec<Said>,
}
impl AgentDraft {
    fn validate(&self) -> Result<()> {
        let plain = |text: &str| {
            !text
                .chars()
                .any(|c| c.is_control() && c != '\n' && c != '\t')
        };
        ensure!(uuid::Uuid::parse_str(&self.id).is_ok(), "Invalid agent ID");
        ensure!(
            self.name.len() <= 80 && self.role.chars().count() <= 80,
            "The name or job title is too long"
        );
        ensure!(
            self.avatar.len() <= 64
                && self
                    .avatar
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-'),
            "Invalid picture"
        );
        ensure!(
            self.instructions.len() <= INSTRUCTIONS_LIMIT && plain(&self.instructions),
            "The ground rules are too long"
        );
        ensure!(
            ["everyone", "selected"].contains(&self.audience.as_str()),
            "Choose who can talk to this agent"
        );
        ensure!(
            self.people.len() <= 200
                && self
                    .people
                    .iter()
                    .all(|id| !id.is_empty() && id.len() <= 200),
            "Choose at most 200 people"
        );
        ensure!(self.step <= 2, "Unknown step");
        ensure!(
            self.skills.len() <= 20
                && self
                    .skills
                    .iter()
                    .all(|s| uuid::Uuid::parse_str(&s.id).is_ok()
                        && s.name.len() <= 64
                        && s.description.len() <= 1024
                        && s.steps.len() <= 20_000),
            "There are too many skills, or one is too long"
        );
        ensure!(
            self.library.len() <= 20
                && self.library.iter().all(|s| !s.id.is_empty()
                    && s.id.len() <= 200
                    && s.name.len() <= 64
                    && s.description.len() <= 1024),
            "There are too many skills"
        );
        ensure!(
            self.messages.len() <= 40
                && self
                    .messages
                    .iter()
                    .all(|m| ["person", "agent"].contains(&m.role.as_str()) && m.text.len() <= 8000),
            "The setup chat is too long to keep"
        );
        Ok(())
    }
}

async fn ready(db: &impl ConnectionTrait) -> Result<bool> {
    Ok(query(
        db,
        "SELECT to_regclass('relay_agent_drafts') IS NOT NULL AS ready",
        vec![],
    )
    .await?
    .context("Missing schema check")?
    .try_get("", "ready")?)
}

impl PgStore {
    /// The member's own unfinished agent here, if they have one.
    pub async fn agent_draft(&self, user: &User, w: &str) -> Result<Value> {
        role(&self.db, w, &user.id).await?;
        if !ready(&self.db).await? {
            return Ok(json!({"draft": null}));
        }
        let row = query(
            &self.db,
            "SELECT draft FROM relay_agent_drafts WHERE workspace=$1 AND user_id=$2",
            vec![w.into(), user.id.clone().into()],
        )
        .await?;
        Ok(match row {
            Some(row) => json!({"draft": row.try_get::<Value>("", "draft")?}),
            None => json!({"draft": null}),
        })
    }

    /// Keeps the member's unfinished agent, in place of the one kept before.
    /// Nothing is created: no agent, no skill, no conversation.
    pub async fn save_agent_draft(&self, user: &User, w: &str, input: &AgentDraft) -> Result<()> {
        input.validate()?;
        let tx = self.workspace_tx(w).await?;
        role(&tx, w, &user.id).await?;
        ensure!(ready(&tx).await?, "Drafts are not available yet");
        execute(
            &tx,
            "INSERT INTO relay_agent_drafts(workspace,user_id,draft) VALUES($1,$2,$3)
             ON CONFLICT(workspace,user_id) DO UPDATE SET draft=EXCLUDED.draft,updated_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint",
            vec![w.into(), user.id.clone().into(), serde_json::to_value(input)?.into()],
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Drops the member's unfinished agent once it is created or cancelled.
    /// A newer draft is kept: only the one named `id` goes.
    pub async fn discard_agent_draft(&self, user: &User, w: &str, id: &str) -> Result<()> {
        let tx = self.workspace_tx(w).await?;
        role(&tx, w, &user.id).await?;
        if ready(&tx).await? {
            execute(
                &tx,
                "DELETE FROM relay_agent_drafts WHERE workspace=$1 AND user_id=$2 AND draft->>'id'=$3",
                vec![w.into(), user.id.clone().into(), id.into()],
            )
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }
}

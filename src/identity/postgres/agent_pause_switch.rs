//! Pause or resume one agent from Activity (people-and-agents-first, Phase 1b).
//! The agent's owner and admins may do it (schema 122: one owner per agent).
//! An agent with no owner stays paused until an admin gives it one. The effect is the same as the pause switch
//! on the Agents page: the agent takes no new work, and work people may no longer
//! ask for stops.
use super::*;
use serde_json::json;

impl PgStore {
    pub(crate) async fn agent_pause_set(
        &self,
        user: &User,
        w: &str,
        id: &str,
        paused: bool,
    ) -> Result<Value> {
        let tx = self.workspace_tx(w).await?;
        let admin = role(&tx, w, &user.id).await? == "Admin";
        let a = super::agent_profiles::stored(&tx, w, id).await?;
        ensure!(
            admin || a["owner_id"] == user.id,
            "Only the agent's owner or an admin can pause or resume it"
        );
        ensure!(
            paused || !a["owner_id"].is_null(),
            "This agent has no owner. An admin needs to give it one before it can work again."
        );
        let revision = a["revision"].as_i64().unwrap_or(0);
        // Asking for the state it is already in changes nothing, so a repeated
        // click or a lost response is harmless.
        if (a["settings"]["paused"] == true) == paused {
            tx.commit().await?;
            return Ok(json!({"paused": paused, "revision": revision}));
        }
        execute(
            &tx,
            "UPDATE relay_shared_agents SET settings=jsonb_set(settings,'{paused}',to_jsonb($3::boolean)),revision=revision+1 WHERE workspace=$1 AND id=$2",
            vec![w.into(), id.into(), paused.into()],
        )
        .await?;
        let actor = super::member_handover::name(&tx, &user.id).await?;
        let name = a["name"].as_str().unwrap_or("an agent");
        let verb = if paused { "paused" } else { "resumed" };
        super::agent_changes::record(&tx, user, w, id, Some(format!("{actor} {verb} {name}")))
            .await?;
        if paused {
            super::task_revocation::inaccessible(&tx, w).await?;
        }
        tx.commit().await?;
        Ok(json!({"paused": paused, "revision": revision + 1}))
    }
}

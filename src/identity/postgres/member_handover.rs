//! What a person is responsible for, and handing it on when an admin removes them.
//! Their agents and workspace-wide Library items move to the chosen person; an
//! only-me agent moves paused, so the new owner looks at it before it works again.
//! Private pages and items shared with selected people stay as they are: moving
//! them would show the new owner something it was never shared with. Existing safety rules still
//! cancel their pending approvals, switch rules they set up or approve to Block,
//! and pause their schedules and event triggers; those are reported, not moved.
use super::*;
use serde_json::json;

/// Everything removal would move, stop or cancel, for the "before you remove" view.
pub(super) async fn responsibilities(
    db: &impl ConnectionTrait,
    w: &str,
    person: &str,
) -> Result<Value> {
    let agents = json_rows(db, "SELECT jsonb_build_object('id',id,'name',name) AS data FROM relay_shared_agents WHERE workspace=$1 AND owner_id=$2 ORDER BY name,id", vec![w.into(), person.into()]).await?;
    let count = |sql: &'static str| async move {
        query(db, sql, vec![w.into(), person.into()])
            .await?
            .context("Workspace unavailable")?
            .try_get::<i64>("", "count")
            .map_err(anyhow::Error::from)
    };
    Ok(json!({
        "agents": agents,
        "private_agents": count("SELECT count(*) AS count FROM relay_shared_agents WHERE workspace=$1 AND owner_id=$2 AND audience='only_me'").await?,
        "library_items": count("SELECT count(*) AS count FROM relay_library_items WHERE workspace=$1 AND steward=$2 AND visibility='workspace' AND NOT trashed").await?,
        "kept_library_items": count("SELECT count(*) AS count FROM relay_library_items WHERE workspace=$1 AND steward=$2 AND visibility<>'workspace' AND NOT trashed").await?,
        "schedules": count("SELECT count(*) AS count FROM relay_agent_schedules WHERE workspace=$1 AND principal=$2 AND state='enabled'").await?,
        "event_triggers": count("SELECT count(*) AS count FROM relay_event_triggers WHERE workspace=$1 AND principal=$2 AND state<>'blocked'").await?,
        "approvals": count("SELECT count(*) AS count FROM relay_control_actions a WHERE a.workspace=$1 AND a.state='pending' AND (a.approver=$2 OR EXISTS(SELECT 1 FROM relay_control_rules r WHERE r.workspace=a.workspace AND r.agent=a.agent AND r.connection=a.connection AND r.operation=a.operation AND r.configured_by=$2))").await?,
        "tool_rules": count("SELECT count(*) AS count FROM relay_control_rules WHERE workspace=$1 AND mode<>'block' AND (configured_by=$2 OR approver=$2)").await?,
        "invitations": count("SELECT count(*) AS count FROM relay_invitations WHERE workspace=$1 AND inviter=$2 AND status='pending'").await?,
    }))
}

/// Moves agents and Library items to `heir` and cancels invitations `person` sent.
/// Returns provider invitation IDs to revoke after commit.
pub(super) async fn hand_over(
    tx: &impl ConnectionTrait,
    w: &str,
    person: &str,
    heir: &str,
) -> Result<Vec<String>> {
    // The new owner also manages the agents they take over.
    execute(tx, "INSERT INTO relay_agent_managers(workspace,agent,user_id,added_by,at) SELECT workspace,id,$3,$3,$4 FROM relay_shared_agents WHERE workspace=$1 AND owner_id=$2 ON CONFLICT DO NOTHING", vec![w.into(), person.into(), heir.into(), now().into()]).await?;
    execute(tx, "UPDATE relay_shared_agents SET owner_id=$3,revision=revision+1,settings=CASE WHEN audience='only_me' THEN jsonb_set(settings,'{paused}','true') ELSE settings END WHERE workspace=$1 AND owner_id=$2", vec![w.into(), person.into(), heir.into()]).await?;
    super::agent_audience::keep_a_manager(tx, w, person).await?;
    execute(tx, "UPDATE relay_library_items SET steward=$3,access_revision=access_revision+1 WHERE workspace=$1 AND steward=$2 AND visibility='workspace'", vec![w.into(), person.into(), heir.into()]).await?;
    let cancelled = rows(tx, "UPDATE relay_invitations SET status='cancelled' WHERE workspace=$1 AND inviter=$2 AND status='pending' RETURNING provider", vec![w.into(), person.into()]).await?;
    Ok(cancelled
        .iter()
        .filter_map(|row| row.try_get::<Option<String>>("", "provider").ok().flatten())
        .collect())
}

pub(super) async fn name(db: &impl ConnectionTrait, user: &str) -> Result<String> {
    query(
        db,
        "SELECT COALESCE(display_name,email) AS name FROM relay_users WHERE id=$1",
        vec![user.into()],
    )
    .await?
    .context("Person unavailable")?
    .try_get("", "name")
    .map_err(Into::into)
}

fn plural(count: i64, one: &str, many: &str) -> Option<String> {
    (count > 0).then(|| format!("{count} {}", if count == 1 { one } else { many }))
}

/// One readable sentence for the audit log, from the counts taken before removal.
/// `actor` is the admin who removed the person, or `None` when they left on their own.
pub(super) fn removal_summary(
    actor: Option<&str>,
    person: &str,
    heir: &str,
    before: &Value,
) -> String {
    let n = |key: &str| before[key].as_i64().unwrap_or(0);
    let agents = before["agents"].as_array().map_or(0, |a| a.len() as i64);
    let mut parts = vec![match actor {
        Some(actor) => format!("{actor} removed {person}"),
        None => format!("{person} left the workspace"),
    }];
    let moved: Vec<String> = [
        plural(agents, "agent", "agents"),
        plural(n("library_items"), "Library item", "Library items"),
    ]
    .into_iter()
    .flatten()
    .collect();
    if !moved.is_empty() {
        parts.push(format!("{} moved to {heir}", moved.join(" and ")));
    }
    let paused: Vec<String> = [
        plural(n("private_agents"), "only-me agent", "only-me agents"),
        plural(n("schedules"), "schedule", "schedules"),
        plural(n("event_triggers"), "event trigger", "event triggers"),
    ]
    .into_iter()
    .flatten()
    .collect();
    if !paused.is_empty() {
        parts.push(format!("{} paused for review", paused.join(" and ")));
    }
    for (key, one, many, verb) in [
        (
            "approvals",
            "waiting approval",
            "waiting approvals",
            "cancelled",
        ),
        ("tool_rules", "tool rule", "tool rules", "switched to Block"),
        ("invitations", "invitation", "invitations", "cancelled"),
        (
            "kept_library_items",
            "private or selected Library item",
            "private or selected Library items",
            "left with their sharing unchanged",
        ),
    ] {
        if let Some(text) = plural(n(key), one, many) {
            parts.push(format!("{text} {verb}"));
        }
    }
    parts.join("; ")
}

impl PgStore {
    pub async fn member_responsibilities(
        &self,
        user: &User,
        w: &str,
        person: &str,
    ) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        // People may see their own, before leaving.
        if person != user.id {
            manager(&tx, w, &user.id).await?;
        }
        role(&tx, w, person).await?;
        let result = responsibilities(&tx, w, person).await?;
        tx.commit().await?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_names_what_moved_paused_and_stopped() {
        let before = json!({"agents":[{"id":"a"},{"id":"b"}],"library_items":1,"schedules":2,"event_triggers":0,"approvals":1,"tool_rules":0,"invitations":3,"private_agents":0,"kept_library_items":0});
        assert_eq!(
            removal_summary(Some("Arjun Mehta"), "Ravi Kumar", "Meera Iyer", &before),
            "Arjun Mehta removed Ravi Kumar; 2 agents and 1 Library item moved to Meera Iyer; 2 schedules paused for review; 1 waiting approval cancelled; 3 invitations cancelled"
        );
        let empty = json!({"agents":[],"library_items":0,"schedules":0,"event_triggers":0,"approvals":0,"tool_rules":0,"invitations":0});
        assert_eq!(removal_summary(Some("A"), "B", "A", &empty), "A removed B");
        let one = json!({"agents":[{"id":"a"}],"library_items":0});
        assert_eq!(
            removal_summary(None, "Ravi Kumar", "Arjun Mehta", &one),
            "Ravi Kumar left the workspace; 1 agent moved to Arjun Mehta"
        );
    }
}

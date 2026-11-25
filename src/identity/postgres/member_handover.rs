//! What a person is responsible for, and handing it on when an admin removes them.
//! Their workspace-wide Library items move to the person taking over. Their
//! agents move to the new owner the admin chose (an only-me agent moves paused,
//! so the new owner looks at it before it works again); with none chosen, or
//! when the person leaves on their own, their agents have no owner and pause
//! until an admin gives them one (schema 122).
//! Private pages and items shared with selected people stay as they are: moving
//! them would show the new owner something it was never shared with. Their open
//! repeating tasks (a rule or a booked follow-up) pause, since every run is for
//! them, and existing safety rules block their event triggers; those are reported,
//! not moved. Pending approvals go to the agent's new owner, or to admins while it
//! has none.
use super::*;
use serde_json::json;

/// The person's (`$2`) open tasks that would run again on their own: a rule, or a
/// follow-up or next run booked. Pausing one cancels what is booked.
const REPEATING: &str = "t.workspace=$1 AND t.requester=$2 AND t.outcome IS NULL AND t.paused_ms IS NULL
  AND (t.repeat IS NOT NULL OR EXISTS(SELECT 1 FROM relay_task_wakeups k WHERE k.workspace=t.workspace AND k.task=t.id AND k.state='pending'))";
const LEFT: &str = "Paused because the person it was for left the workspace.";

/// Everything removal would move, stop or cancel, for the "before you remove" view.
pub(super) async fn responsibilities(
    db: &impl ConnectionTrait,
    w: &str,
    person: &str,
) -> Result<Value> {
    // Since schema 122 an agent may have several owners: only the ones this
    // person owns alone pause or move when they go (`alone`).
    let owners = super::agent_owners::ready(db).await?;
    let mine = if owners {
        "relay_agent_owns(a.workspace,a.id,$2)"
    } else {
        "a.owner_id=$2"
    };
    let alone = if owners {
        "NOT EXISTS(SELECT 1 FROM relay_agent_owners o WHERE o.workspace=a.workspace AND o.agent=a.id AND o.user_id<>$2)"
    } else {
        "TRUE"
    };
    let agents = json_rows(db, &format!("SELECT jsonb_build_object('id',a.id,'name',a.name,'alone',{alone}) AS data FROM relay_shared_agents a WHERE a.workspace=$1 AND {mine} ORDER BY a.name,a.id"), vec![w.into(), person.into()]).await?;
    let private_agents = query(db, &format!("SELECT count(*) AS count FROM relay_shared_agents a WHERE a.workspace=$1 AND {mine} AND {alone} AND a.audience='only_me'"), vec![w.into(), person.into()])
        .await?.context("Workspace unavailable")?.try_get::<i64>("", "count")?;
    let count = |sql: String| async move {
        query(db, &sql, vec![w.into(), person.into()])
            .await?
            .context("Workspace unavailable")?
            .try_get::<i64>("", "count")
            .map_err(anyhow::Error::from)
    };
    // Schemas before 144 have no task rules or pauses.
    let repeating = if super::tasks::wakeups::lifetimes(db).await? {
        count(format!(
            "SELECT count(*) AS count FROM relay_tasks t WHERE {REPEATING}"
        ))
        .await?
    } else {
        0
    };
    Ok(json!({
        "agents": agents,
        "private_agents": private_agents,
        "library_items": count("SELECT count(*) AS count FROM relay_library_items WHERE workspace=$1 AND steward=$2 AND visibility='workspace' AND NOT trashed".into()).await?,
        "kept_library_items": count("SELECT count(*) AS count FROM relay_library_items WHERE workspace=$1 AND steward=$2 AND visibility<>'workspace' AND NOT trashed".into()).await?,
        "repeating_tasks": repeating,
        "event_triggers": count("SELECT count(*) AS count FROM relay_event_triggers WHERE workspace=$1 AND principal=$2 AND state<>'blocked'".into()).await?,
        "invitations": count("SELECT count(*) AS count FROM relay_invitations WHERE workspace=$1 AND inviter=$2 AND status='pending'".into()).await?,
    }))
}

/// Moves Library items to `heir`, agents to `new_owner` (or leaves them without
/// an owner, paused), pauses `person`'s repeating tasks and cancels invitations
/// they sent. Returns provider invitation IDs to revoke after commit.
pub(super) async fn hand_over(
    tx: &impl ConnectionTrait,
    w: &str,
    person: &str,
    heir: &str,
    new_owner: Option<&str>,
) -> Result<Vec<String>> {
    super::agent_owners::owner_leaves(tx, w, person, new_owner, heir).await?;
    execute(tx, "UPDATE relay_library_items SET steward=$3,access_revision=access_revision+1 WHERE workspace=$1 AND steward=$2 AND visibility='workspace'", vec![w.into(), person.into(), heir.into()]).await?;
    if super::tasks::wakeups::lifetimes(tx).await? {
        let paused = rows(tx, &format!("UPDATE relay_tasks t SET paused_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint,paused_reason=$3 WHERE {REPEATING} RETURNING t.id"),
            vec![w.into(), person.into(), LEFT.into()]).await?;
        let ids: Vec<String> = paused
            .iter()
            .filter_map(|row| row.try_get("", "id").ok())
            .collect();
        execute(tx, "UPDATE relay_task_wakeups SET state='cancelled' WHERE workspace=$1 AND state='pending' AND task IN (SELECT jsonb_array_elements_text($2::jsonb))",
            vec![w.into(), json!(ids).into()]).await?;
    }
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
    new_owner: Option<&str>,
    before: &Value,
) -> String {
    let n = |key: &str| before[key].as_i64().unwrap_or(0);
    let listed = before["agents"].as_array().cloned().unwrap_or_default();
    // Agents with other owners stay with them; only ones owned alone move or pause.
    let shared = listed.iter().filter(|a| a["alone"] == false).count() as i64;
    let alone = listed.len() as i64 - shared;
    let mut parts = vec![match actor {
        Some(actor) => format!("{actor} removed {person}"),
        None => format!("{person} left the workspace"),
    }];
    if let Some(shared) = plural(shared, "agent stays", "agents stay") {
        parts.push(format!("{shared} with their other owners"));
    }
    let agents = plural(alone, "agent", "agents");
    let library = plural(n("library_items"), "Library item", "Library items");
    if new_owner == Some(heir) {
        // One person takes both.
        let moved: Vec<String> = [agents, library].into_iter().flatten().collect();
        if !moved.is_empty() {
            parts.push(format!("{} moved to {heir}", moved.join(" and ")));
        }
    } else {
        if let Some(agents) = agents {
            parts.push(match new_owner {
                Some(owner) => format!("{agents} moved to {owner}"),
                None => format!("{agents} paused until an admin gives them an owner"),
            });
        }
        if let Some(library) = library {
            parts.push(format!("{library} moved to {heir}"));
        }
    }
    let paused: Vec<String> = [
        new_owner.and(plural(
            n("private_agents"),
            "only-me agent",
            "only-me agents",
        )),
        plural(n("repeating_tasks"), "repeating task", "repeating tasks"),
        plural(n("event_triggers"), "event trigger", "event triggers"),
    ]
    .into_iter()
    .flatten()
    .collect();
    if !paused.is_empty() {
        parts.push(format!("{} paused for review", paused.join(" and ")));
    }
    for (key, one, many, verb) in [
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
        let before = json!({"agents":[{"id":"a"},{"id":"b"}],"library_items":1,"repeating_tasks":2,"event_triggers":0,"invitations":3,"private_agents":0,"kept_library_items":0});
        assert_eq!(
            removal_summary(
                Some("Arjun Mehta"),
                "Ravi Kumar",
                "Meera Iyer",
                Some("Meera Iyer"),
                &before
            ),
            "Arjun Mehta removed Ravi Kumar; 2 agents and 1 Library item moved to Meera Iyer; 2 repeating tasks paused for review; 3 invitations cancelled"
        );
        let empty = json!({"agents":[],"library_items":0,"repeating_tasks":0,"event_triggers":0,"invitations":0});
        assert_eq!(
            removal_summary(Some("A"), "B", "A", None, &empty),
            "A removed B"
        );
        let one = json!({"agents":[{"id":"a"},{"id":"b","alone":false}],"library_items":0});
        assert_eq!(
            removal_summary(None, "Ravi Kumar", "Arjun Mehta", None, &one),
            "Ravi Kumar left the workspace; 1 agent stays with their other owners; 1 agent paused until an admin gives them an owner"
        );
        let both = json!({"agents":[{"id":"a"}],"library_items":2});
        assert_eq!(
            removal_summary(
                Some("Arjun Mehta"),
                "Ravi Kumar",
                "Arjun Mehta",
                Some("Meera Iyer"),
                &both
            ),
            "Arjun Mehta removed Ravi Kumar; 1 agent moved to Meera Iyer; 2 Library items moved to Arjun Mehta"
        );
    }
}

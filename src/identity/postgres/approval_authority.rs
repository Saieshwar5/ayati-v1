//! Who may see and decide an "Ask first" action. Deciding is separate from
//! configuring an agent or joining a channel.
use super::*;

// Approval visibility must not grant or require the separate right to request agent work.
pub(super) async fn agent_visible(
    db: &impl ConnectionTrait,
    user: &str,
    w: &str,
    agent: &str,
) -> Result<bool> {
    role(db, w, user).await?;
    let a = super::agent_profiles::stored(db, w, agent).await?;
    let private = a["audience"] == "only_me";
    ensure!(
        !private || a["owner_id"] == user,
        "Private agent approval unavailable"
    );
    Ok(private)
}

/// `person` may still decide this agent's "Ask first" actions: they are in the
/// workspace and are the approver the current rules name (the agent's owner).
pub(super) async fn require(
    db: &impl ConnectionTrait,
    w: &str,
    agent: &str,
    person: &str,
) -> Result<()> {
    role(db, w, person)
        .await
        .context("Approver authority changed")?;
    let a = super::agent_profiles::stored(db, w, agent).await?;
    ensure!(a["owner_id"] == person, "Approver authority changed");
    Ok(())
}

/// Whether `person` manages `agent` (decides its "Ask first" actions).
pub(super) async fn manages(
    db: &impl ConnectionTrait,
    w: &str,
    agent: &str,
    person: &str,
) -> Result<bool> {
    Ok(query(
        db,
        "SELECT user_id FROM relay_agent_managers WHERE workspace=$1 AND agent=$2 AND user_id=$3",
        vec![w.into(), agent.into(), person.into()],
    )
    .await?
    .is_some())
}

/// The managers who could decide `a` now: they can see the request, and they
/// are not its requester when the action needs someone else.
pub(super) async fn deciders(db: &impl ConnectionTrait, w: &str, a: &Value) -> Result<Vec<Value>> {
    let people = json_rows(db,"SELECT jsonb_build_object('id',u.id,'name',COALESCE(u.display_name,u.email)) AS data FROM relay_agent_managers g JOIN relay_users u ON u.id=g.user_id WHERE g.workspace=$1 AND g.agent=$2 ORDER BY g.at,g.user_id",vec![w.into(),a["agent"].as_str().unwrap_or_default().into()]).await?;
    let mut eligible = vec![];
    for person in people {
        let user = User {
            id: person["id"].as_str().unwrap().into(),
            email: String::new(),
            display_name: None,
        };
        if !(a["needs_other"] == true && a["requester"] == user.id)
            && super::approval_delivery::viewer(db, &user, w, a)
                .await
                .is_ok()
        {
            eligible.push(person);
        }
    }
    Ok(eligible)
}

/// When admins may step in: after the workspace's backup delay (seconds since epoch).
pub(super) async fn backup_at(db: &impl ConnectionTrait, w: &str, a: &Value) -> Result<i64> {
    let hours: i32 = query(
        db,
        "SELECT approval_backup_hours FROM relay_workspaces WHERE id=$1",
        vec![w.into()],
    )
    .await?
    .context("Workspace unavailable")?
    .try_get("", "approval_backup_hours")?;
    Ok(a["created"].as_i64().unwrap_or_default() + i64::from(hours) * 3600)
}

/// `user` may decide `a` now (D231): any manager who can see the request; an
/// admin once the backup delay has passed or when no manager can decide; never
/// the requester when the action needs someone else.
pub(super) async fn may_decide(
    db: &impl ConnectionTrait,
    user: &User,
    w: &str,
    a: &Value,
) -> Result<()> {
    super::approval_delivery::viewer(db, user, w, a).await?;
    ensure!(
        !(a["needs_other"] == true && a["requester"] == user.id),
        "Someone other than the person who asked must approve this action"
    );
    let agent = a["agent"].as_str().context("Missing agent")?;
    if manages(db, w, agent, &user.id).await? {
        return Ok(());
    }
    ensure!(
        role(db, w, &user.id).await? == "Admin",
        "Only this agent's managers can decide"
    );
    ensure!(
        now() >= backup_at(db, w, a).await? || deciders(db, w, a).await?.is_empty(),
        "The agent's managers decide first; admins can decide if nobody has after the backup delay"
    );
    Ok(())
}

/// SQL condition, with the person as `$2`, for actions they may be asked to
/// decide: they manage the agent, or they are an admin and the backup delay has
/// passed or the agent has no managers. Rust rechecks with `may_decide`.
/// Upgrade tests read this on older schemas, where the approver alone decides.
pub(super) async fn decider_clause(db: &impl ConnectionTrait) -> Result<&'static str> {
    let schema_47 = query(
        db,
        "SELECT to_regclass('relay_connector_rules') IS NOT NULL AS present",
        vec![],
    )
    .await?
    .context("Schema unavailable")?
    .try_get::<bool>("", "present")?;
    Ok(if schema_47 {
        "(a.approver=$2 OR EXISTS(SELECT 1 FROM relay_agent_managers g WHERE g.workspace=a.workspace AND g.agent=a.agent AND g.user_id=$2) OR EXISTS(SELECT 1 FROM relay_members m JOIN relay_workspaces ws ON ws.id=m.workspace WHERE m.workspace=a.workspace AND m.user_id=$2 AND m.role='Admin' AND (a.created+ws.approval_backup_hours*3600<=floor(extract(epoch FROM clock_timestamp()))::bigint OR NOT EXISTS(SELECT 1 FROM relay_agent_managers g WHERE g.workspace=a.workspace AND g.agent=a.agent))))"
    } else {
        "a.approver=$2"
    })
}

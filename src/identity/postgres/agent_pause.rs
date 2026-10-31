//! Pause all agents: an admin's emergency switch for the whole workspace.
//!
//! Pausing stops every unfinished request at once (the same stop as a person's
//! Stop: running attempts end at their next lease renewal and pending approvals
//! are invalidated) and refuses new agent work until an admin resumes. While
//! paused, schedules skip their runs and arriving events are not handled; neither
//! is disabled. This is a separate gate from agent access on purpose: lost access
//! permanently blocks schedules and triggers, and a pause must not.
use super::*;
use serde::Deserialize;
use serde_json::json;

#[cfg(test)]
#[path = "agent_pause_tests.rs"]
mod tests;

/// The note shown on every request the pause stopped.
const STOPPED: &str = "Stopped because all agents were paused.";
const NOT_SENT: &str = "All agents were paused; no pending operation was dispatched.";

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PauseInput {
    /// Why, shown to everyone in the workspace; optional.
    pub note: Option<String>,
}

/// Whether all agents are paused. Reads the column through `to_jsonb`, so current
/// code also runs on schemas before 51.
pub(crate) async fn paused(db: &impl ConnectionTrait, w: &str) -> Result<bool> {
    Ok(query(db, "SELECT to_jsonb(w)->>'agents_paused_ms' IS NOT NULL AS paused FROM relay_workspaces w WHERE w.id=$1", vec![w.into()])
        .await?
        .map(|row| row.try_get::<bool>("", "paused"))
        .transpose()?
        .unwrap_or(false))
}

/// Refuses new agent work while paused, with an error schedules and triggers
/// recognise (they skip rather than block).
pub(crate) async fn ensure_running(db: &impl ConnectionTrait, w: &str) -> Result<()> {
    if paused(db, w).await? {
        return Err(crate::identity::chat::model::AgentsPaused.into());
    }
    Ok(())
}

/// Tells every open tab in the workspace, through the existing change stream.
async fn announce(tx: &DatabaseTransaction, w: &str) -> Result<()> {
    let cursor: i64 = query(
        tx,
        "INSERT INTO relay_sync_clocks(workspace,cursor) VALUES($1,1)
        ON CONFLICT(workspace) DO UPDATE SET cursor=relay_sync_clocks.cursor+1 RETURNING cursor",
        vec![w.into()],
    )
    .await?
    .context("Change clock unavailable")?
    .try_get("", "cursor")?;
    execute(
        tx,
        "INSERT INTO relay_sync_events(workspace,cursor,kind) VALUES($1,$2,'agents')",
        vec![w.into(), cursor.into()],
    )
    .await?;
    query(
        tx,
        "SELECT pg_notify('relay_message_changes',$1)::text AS sent",
        vec![w.into()],
    )
    .await?;
    Ok(())
}

async fn state(db: &impl ConnectionTrait, w: &str, admin: bool) -> Result<Value> {
    let row = query(db, "SELECT (to_jsonb(w)->>'agents_paused_ms')::bigint AS since,to_jsonb(w)->>'agents_paused_note' AS note,
        COALESCE(u.display_name,u.email,CASE WHEN to_jsonb(w)->>'agents_paused_by' IS NOT NULL THEN 'Former member' END) AS by,
        (SELECT count(*) FROM relay_chat_jobs j WHERE j.workspace=w.id AND j.state<>'finished') AS active
        FROM relay_workspaces w LEFT JOIN relay_users u ON u.id=to_jsonb(w)->>'agents_paused_by' WHERE w.id=$1", vec![w.into()])
        .await?
        .context("Workspace unavailable")?;
    let since: Option<i64> = row.try_get("", "since")?;
    Ok(json!({
        "paused": since.is_some(), "since_ms": since,
        "by": row.try_get::<Option<String>>("", "by")?, "note": row.try_get::<Option<String>>("", "note")?,
        // How many requests a pause would stop; only admins can pause.
        "active": if admin { Some(row.try_get::<i64>("", "active")?) } else { None },
    }))
}

impl PgStore {
    /// Whether all agents are paused, for every member's banner.
    pub(crate) async fn agents_pause_state(&self, user: &User, w: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        let admin = role(&tx, w, &user.id).await? == "Admin";
        let data = state(&tx, w, admin).await?;
        tx.commit().await?;
        Ok(data)
    }

    /// Pauses all agents and stops every unfinished request. Pausing again while
    /// paused changes nothing and keeps the first time, person and note.
    pub(crate) async fn pause_agents(
        &self,
        user: &User,
        w: &str,
        input: &PauseInput,
    ) -> Result<Value> {
        let note = input
            .note
            .as_deref()
            .map(str::trim)
            .filter(|n| !n.is_empty());
        ensure!(
            note.is_none_or(|n| n.chars().count() <= 300 && !n.chars().any(char::is_control)),
            "Keep the reason under 300 characters"
        );
        let tx = self.workspace_tx(w).await?;
        manager(&tx, w, &user.id).await?;
        if !paused(&tx, w).await? {
            execute(&tx, "UPDATE relay_workspaces SET agents_paused_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint,agents_paused_by=$2,agents_paused_note=$3 WHERE id=$1",
                vec![w.into(), user.id.clone().into(), note.map(str::to_owned).into()]).await?;
            let replies = rows(&tx, "SELECT reply FROM relay_chat_jobs WHERE workspace=$1 AND state<>'finished' ORDER BY sequence", vec![w.into()]).await?;
            let mut stopped = 0;
            for row in replies {
                let reply: String = row.try_get("", "reply")?;
                stopped += super::super::assignments::stop_job(&tx, w, &reply, STOPPED, NOT_SENT)
                    .await? as usize;
            }
            let count = match stopped {
                0 => String::new(),
                1 => " and stopped 1 request".into(),
                n => format!(" and stopped {n} requests"),
            };
            let reason = note.map(|n| format!(": {n}")).unwrap_or_default();
            audit_with_summary(
                &tx,
                &user.id,
                Some(w),
                "agents_paused",
                w,
                &format!("paused all agents{count}{reason}"),
            )
            .await?;
            announce(&tx, w).await?;
        }
        let data = state(&tx, w, true).await?;
        tx.commit().await?;
        Ok(data)
    }

    /// Lets agents take work again. Agents paused on their own stay paused, and
    /// stopped requests don't restart by themselves.
    pub(crate) async fn resume_agents(&self, user: &User, w: &str) -> Result<Value> {
        let tx = self.workspace_tx(w).await?;
        manager(&tx, w, &user.id).await?;
        if execute(&tx, "UPDATE relay_workspaces SET agents_paused_ms=NULL,agents_paused_by=NULL,agents_paused_note=NULL
            WHERE id=$1 AND agents_paused_ms IS NOT NULL", vec![w.into()]).await? == 1 {
            audit_with_summary(&tx, &user.id, Some(w), "agents_resumed", w, "resumed all agents").await?;
            announce(&tx, w).await?;
        }
        let data = state(&tx, w, true).await?;
        tx.commit().await?;
        Ok(data)
    }
}

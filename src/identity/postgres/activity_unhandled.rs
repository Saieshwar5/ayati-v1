//! Events that arrived but no agent acted on: the trigger was paused or blocked,
//! all agents were paused, or access changed (people-and-agents-first, Phase 1).
//! They never became requests, so History's request list can't show them; this is
//! their own list, paged by the event's arrival order.
//!
//! Who sees one: the person who set up the trigger, the agent's managers and admins.
//! Only the trigger's name, the agent, the time and the recorded reason are shown,
//! never the event's contents, which may come from a source the viewer can't read.
use super::*;
use serde_json::json;

/// `$1` workspace, `$2` person, `$3` admin, `$4` arrived from (ms), `$5` before
/// (sequence), `$6` scope, `$7` agent.
const UNHANDLED: &str = "FROM relay_event_deliveries d
 JOIN relay_event_inbox i ON i.workspace=d.workspace AND i.source=d.source AND i.id=d.event_id
 JOIN relay_event_triggers t ON t.workspace=d.workspace AND t.id=d.trigger_id
 JOIN relay_shared_agents a ON a.workspace=t.workspace AND a.id=t.agent
 WHERE d.workspace=$1 AND d.state='cancelled'
  AND (t.principal=$2 OR $3
   OR EXISTS(SELECT 1 FROM relay_agent_managers g WHERE g.workspace=t.workspace AND g.agent=t.agent AND g.user_id=$2))
  AND ($6::text IS DISTINCT FROM 'mine' OR t.principal=$2
   OR EXISTS(SELECT 1 FROM relay_agent_managers g WHERE g.workspace=t.workspace AND g.agent=t.agent AND g.user_id=$2))
  AND ($7::text IS NULL OR t.agent=$7)
  AND i.received_ms>=$4 AND i.sequence<$5";

fn values(
    w: &str,
    user: &str,
    admin: bool,
    from: i64,
    before: i64,
    scope: Option<&str>,
    agent: Option<&str>,
) -> Vec<sea_orm::Value> {
    vec![
        w.into(),
        user.into(),
        admin.into(),
        from.into(),
        before.into(),
        scope.map(str::to_string).into(),
        agent.map(str::to_string).into(),
    ]
}

impl PgStore {
    /// One page of events that weren't handled, newest first; `next_before` is the
    /// cursor for the next page.
    pub(crate) async fn activity_unhandled(
        &self,
        user: &User,
        w: &str,
        from: Option<i64>,
        before: Option<i64>,
        scope: Option<&str>,
        agent: Option<&str>,
    ) -> Result<Value> {
        super::activity::valid_scope(scope)?;
        ensure!(before.is_none_or(|b| b > 0), "Invalid cursor");
        let tx = self.activity_tx(w, false).await?;
        let admin = role(&tx, w, &user.id).await? == "Admin";
        let sql = format!(
            "SELECT jsonb_build_object('id',d.id,'sequence',i.sequence,'at',i.received_ms,'trigger',t.id,
              'title',COALESCE(NULLIF(t.source_definition->>'name',''),'Connected event'),
              'reason',NULLIF(d.detail,''),'agent',a.id,'agent_name',a.name,'agent_avatar',a.settings->>'avatar') AS data
             {UNHANDLED} ORDER BY i.sequence DESC LIMIT 26"
        );
        let values = values(
            w,
            &user.id,
            admin,
            from.unwrap_or(0),
            before.unwrap_or(i64::MAX),
            scope,
            agent,
        );
        let mut items = json_rows(&tx, &sql, values).await?;
        tx.commit().await?;
        let more = items.len() > 25;
        items.truncate(25);
        let next = if more {
            items.last().and_then(|i| i["sequence"].as_i64())
        } else {
            None
        };
        Ok(json!({"items": items, "next_before": next}))
    }
}

/// How many events weren't handled since `since_ms`, for the line on Now.
pub(super) async fn count_since(
    tx: &DatabaseTransaction,
    w: &str,
    user: &str,
    admin: bool,
    since_ms: i64,
    scope: Option<&str>,
) -> Result<i64> {
    Ok(query(
        tx,
        &format!("SELECT count(*) AS n {UNHANDLED}"),
        values(w, user, admin, since_ms, i64::MAX, scope, None),
    )
    .await?
    .context("Count unavailable")?
    .try_get("", "n")?)
}

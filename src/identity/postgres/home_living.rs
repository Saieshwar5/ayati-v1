//! Living panels (people-and-agents-first, Phase 3 HP2): a Home panel with an update
//! agreement also updates when its agent finishes work that touched one of the
//! panel's source connections, or when an event arrives on one. Requests merge into
//! one wanted row per panel; the scheduler loop starts at most one update per panel
//! every `min_gap_seconds`, as the panel's owner, with the same checks as Update now.
use super::super::*;
use crate::identity::chat::model::{Job, NewMessage};

#[path = "home_direct.rs"]
mod direct;
#[path = "home_living_migration.rs"]
pub(in crate::identity::postgres) mod migration;
#[cfg(test)]
#[path = "home_living_tests.rs"]
mod tests;

/// Upgrade tests run current code on older schemas.
pub(in crate::identity::postgres) async fn available(db: &impl ConnectionTrait) -> Result<bool> {
    Ok(query(
        db,
        "SELECT to_regclass('relay_home_update_wanted') IS NOT NULL AS ready",
        vec![],
    )
    .await?
    .is_some_and(|r| r.try_get::<bool>("", "ready").unwrap_or(false)))
}

const WANT: &str = "INSERT INTO relay_home_update_wanted(workspace,panel,reason,cause,due_ms)
  SELECT u.workspace,u.panel,$2,$3,(extract(epoch FROM clock_timestamp())*1000)::bigint
  FROM relay_home_maintenance u JOIN relay_home_panels p ON p.workspace=u.workspace AND p.id=u.panel
  WHERE u.workspace=$1 AND u.enabled AND u.follow_sources AND MATCH
  ON CONFLICT(workspace,panel) DO NOTHING";

/// The agent finished a request: its panels whose sources it touched want an update.
/// A panel update itself never asks for another.
pub(in crate::identity::postgres) async fn after_work(
    tx: &DatabaseTransaction,
    job: &Job,
) -> Result<()> {
    if !available(tx).await? {
        return Ok(());
    }
    let sql = WANT.replace("MATCH", "p.agent=$4
        AND NOT EXISTS(SELECT 1 FROM relay_home_update_runs r WHERE r.workspace=$1 AND r.reply=$3)
        AND EXISTS(SELECT 1 FROM relay_control_actions a WHERE a.workspace=$1 AND a.job=$3 AND a.state='completed'
          AND EXISTS(SELECT 1 FROM jsonb_array_elements(u.scopes) s WHERE s->>'connection'=a.connection))");
    execute(
        tx,
        &sql,
        vec![
            job.workspace.clone().into(),
            "work".into(),
            job.reply.clone().into(),
            job.agent.clone().into(),
        ],
    )
    .await?;
    Ok(())
}

/// An event arrived on a connection: panels reading from it want an update.
pub(in crate::identity::postgres) async fn on_event(
    tx: &DatabaseTransaction,
    w: &str,
    connection: &str,
    event: &str,
) -> Result<()> {
    if !available(tx).await? {
        return Ok(());
    }
    let sql = WANT.replace(
        "MATCH",
        "EXISTS(SELECT 1 FROM jsonb_array_elements(u.scopes) s WHERE s->>'connection'=$4)",
    );
    let cause: String = event.chars().take(200).collect();
    execute(
        tx,
        &sql,
        vec![w.into(), "event".into(), cause.into(), connection.into()],
    )
    .await?;
    Ok(())
}

/// The latest completed result of each reviewed read in this update, in order.
pub(in crate::identity::postgres) async fn read_results(
    db: &impl ConnectionTrait,
    job: &Job,
    scopes: &[Value],
) -> Result<Vec<Value>> {
    let mut results = vec![];
    for scope in scopes {
        let row = query(db, "SELECT result FROM relay_control_actions WHERE workspace=$1 AND job=$2 AND connection=$3 AND operation=$4 AND arguments=$5 AND effect='read' AND state='completed' ORDER BY updated DESC LIMIT 1",
            vec![job.workspace.clone().into(), job.reply.clone().into(), scope["connection"].as_str().unwrap_or_default().into(), scope["operation"].as_str().unwrap_or_default().into(), scope["arguments"].clone().into()]).await?;
        results.push(match row {
            Some(row) => row
                .try_get::<Option<Value>>("", "result")?
                .unwrap_or(Value::Null),
            None => Value::Null,
        });
    }
    Ok(results)
}

/// Keep the agent's bindings if they reproduce exactly what this update publishes.
/// Returns why not, for the agent, without failing the update.
pub(in crate::identity::postgres) async fn keep_bindings(
    tx: &DatabaseTransaction,
    job: &Job,
    data: &Value,
    sections: &[Value],
    list: &[crate::panels::bindings::Binding],
) -> Result<std::result::Result<(), String>> {
    if !available(tx).await? {
        return Ok(Err("this server can't keep bindings yet".into()));
    }
    let scopes = data["policy"]["scopes"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if let Err(error) = crate::panels::bindings::validate_all(list, scopes.len(), sections) {
        return Ok(Err(error.to_string()));
    }
    let results = read_results(tx, job, &scopes).await?;
    if let Err(error) = crate::panels::bindings::check_all(sections, list, &results) {
        return Ok(Err(error.to_string()));
    }
    execute(tx, "UPDATE relay_home_maintenance SET bindings=$3 WHERE workspace=$1 AND panel=$2 AND revision=$4",
        vec![job.workspace.clone().into(), data["panel"].as_str().unwrap_or_default().into(), serde_json::to_value(list)?.into(), data["policy"]["revision"].as_i64().unwrap_or(0).into()]).await?;
    Ok(Ok(()))
}

/// A new update of a panel with bindings is run by the controller itself: claimed
/// here, before any executor can see it, and handed back if anything doesn't fit.
pub(in crate::identity::postgres) async fn claim_direct(
    db: &impl ConnectionTrait,
    w: &str,
    panel: &str,
    reply: &str,
) -> Result<()> {
    if !available(db).await? {
        return Ok(());
    }
    execute(db, "UPDATE relay_chat_jobs j SET state='running',claim='direct-'||$4,lease_until=floor(extract(epoch FROM clock_timestamp()))::bigint+180,attempts=attempts+1,started_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint
        WHERE j.workspace=$1 AND j.reply=$3 AND j.state='queued'
        AND EXISTS(SELECT 1 FROM relay_home_maintenance u WHERE u.workspace=$1 AND u.panel=$2 AND u.enabled AND u.bindings IS NOT NULL)",
        vec![w.into(), panel.into(), reply.into(), uuid::Uuid::new_v4().to_string().into()]).await?;
    Ok(())
}

impl PgStore {
    /// Start the panel updates that are wanted and allowed now. Run every second by
    /// the scheduler loop; like schedules, only with pooled executors.
    pub(crate) async fn dispatch_panel_updates(&self) -> Result<usize> {
        if !self.pooled || !available(&self.db).await? {
            return Ok(0);
        }
        let due = rows(&self.db, "SELECT workspace,panel FROM relay_home_update_wanted WHERE due_ms<=(extract(epoch FROM clock_timestamp())*1000)::bigint ORDER BY due_ms LIMIT 20", vec![]).await?;
        let mut started = 0;
        for row in due {
            let w: String = row.try_get("", "workspace")?;
            let panel: String = row.try_get("", "panel")?;
            if self.start_wanted(&w, &panel).await? {
                started += 1;
            }
        }
        Ok(started)
    }

    async fn start_wanted(&self, w: &str, panel: &str) -> Result<bool> {
        let tx = self.workspace_tx(w).await?;
        let Some(row) = query(&tx, "SELECT k.reason,k.cause,p.owner_id,p.agent,p.chat,
              COALESCE(u.enabled AND u.follow_sources,false) AS wanted,u.min_gap_seconds,
              (SELECT max(r.created_ms) FROM relay_home_update_runs r WHERE r.workspace=k.workspace AND r.panel=k.panel) AS last,
              (extract(epoch FROM clock_timestamp())*1000)::bigint AS now
            FROM relay_home_update_wanted k JOIN relay_home_panels p ON p.workspace=k.workspace AND p.id=k.panel
            LEFT JOIN relay_home_maintenance u ON u.workspace=k.workspace AND u.panel=k.panel
            WHERE k.workspace=$1 AND k.panel=$2 FOR UPDATE OF k SKIP LOCKED",
            vec![w.into(), panel.into()]).await? else { return Ok(false) };
        let args = |extra: Vec<sea_orm::Value>| {
            let mut v: Vec<sea_orm::Value> = vec![w.into(), panel.into()];
            v.extend(extra);
            v
        };
        const DROP: &str = "DELETE FROM relay_home_update_wanted WHERE workspace=$1 AND panel=$2";
        const LATER: &str = "UPDATE relay_home_update_wanted SET due_ms=$3,detail=$4 WHERE workspace=$1 AND panel=$2";
        if !row.try_get::<bool>("", "wanted")? {
            execute(&tx, DROP, args(vec![])).await?;
            tx.commit().await?;
            return Ok(false);
        }
        let now: i64 = row.try_get("", "now")?;
        let gap: i64 = row.try_get::<i64>("", "min_gap_seconds")? * 1000;
        // An update already running may have read before this change: one more after it.
        if super::unfinished(&tx, w, panel).await?.is_some() {
            let detail = "Waiting for the current update to finish.";
            execute(&tx, LATER, args(vec![(now + 60_000).into(), detail.into()])).await?;
            tx.commit().await?;
            return Ok(false);
        }
        // At most one update per panel every min_gap_seconds.
        if let Some(last) = row.try_get::<Option<i64>>("", "last")?
            && last + gap > now
        {
            execute(&tx, LATER, args(vec![(last + gap).into(), "".into()])).await?;
            tx.commit().await?;
            return Ok(false);
        }
        let person = User {
            id: row.try_get("", "owner_id")?,
            email: String::new(),
            display_name: None,
        };
        let p = super::super::home_maintenance::owned(&tx, &person, w, panel).await?;
        if let Err(error) = super::super::home_maintenance::authorized(&tx, &person, w, &p).await {
            if error.is::<sea_orm::DbErr>() {
                return Err(error);
            }
            // The panel shows it can't update; its owner reviews it on Home.
            execute(&tx, DROP, args(vec![])).await?;
            tx.commit().await?;
            return Ok(false);
        }
        let reason: String = row.try_get("", "reason")?;
        let cause: String = row.try_get("", "cause")?;
        let input = NewMessage {
            reply_to: None,
            id: uuid::Uuid::new_v4().to_string(),
            content: format!(
                "Refresh my personal Home panel using home_panel_update_context, its reviewed reads and refresh_home_panel. Its sources changed: {}.",
                if reason == "work" {
                    "you finished work that used them"
                } else {
                    "a new event arrived"
                }
            ),
            agent: Some(row.try_get("", "agent")?),
            thread: None,
            attachments: vec![],
        };
        let chat: String = row.try_get("", "chat")?;
        let intake = tx.begin().await?;
        match self.background_in(&intake, &person, w, &chat, &input).await {
            Ok(Some(job)) => {
                intake.commit().await?;
                super::bind(&tx, w, panel, &job).await?;
                execute(&tx, "UPDATE relay_home_update_runs SET reason=$3,cause=$4 WHERE workspace=$1 AND reply=$2", vec![w.into(), job.reply.into(), reason.into(), cause.into()]).await?;
                execute(&tx, DROP, args(vec![])).await?;
                tx.commit().await?;
                Ok(true)
            }
            Ok(None) => {
                intake.rollback().await?;
                execute(&tx, DROP, args(vec![])).await?;
                tx.commit().await?;
                Ok(false)
            }
            Err(error) => {
                intake.rollback().await?;
                if error.is::<sea_orm::DbErr>() {
                    return Err(error);
                }
                let busy = error.is::<crate::identity::chat::model::AgentsPaused>()
                    || error.is::<crate::identity::chat::model::QueueFull>()
                    || error.is::<crate::identity::chat::model::BackgroundQueueFull>();
                if busy {
                    let detail = "Agents are paused or busy; trying again in five minutes.";
                    execute(
                        &tx,
                        LATER,
                        args(vec![(now + 300_000).into(), detail.into()]),
                    )
                    .await?;
                } else {
                    execute(&tx, DROP, args(vec![])).await?;
                }
                tx.commit().await?;
                Ok(false)
            }
        }
    }
}

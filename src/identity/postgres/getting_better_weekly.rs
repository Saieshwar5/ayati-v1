//! The weekly review (Phase 5): a task "Weekly review" for the agent, for the person
//! who turned it on, that runs every week in its own work room and shows only in
//! Activity. Turning it off pauses the task; on again resumes it.
use super::super::super::super::tasks::{create_repeating, rules::Rule, runs::set_paused};
use super::super::super::super::*;
use serde_json::json;

const REVIEW: &str = "Write your weekly review for your owner. Look back over the last seven days of your requests and tasks, then call write_review once: done (what you finished), went_wrong (what failed or needed a person, and why), learned (what you'd do differently) and, if useful, suggestion (one change to your instructions or skills for a manager to approve). Plain, short sentences; no new work.";

/// The weekly review needs its task link (schema 147).
async fn ready(db: &impl ConnectionTrait) -> Result<bool> {
    Ok(query(db, "SELECT EXISTS(SELECT 1 FROM pg_attribute WHERE attrelid=to_regclass('relay_review_routines') AND attname='task' AND NOT attisdropped) AS ok", vec![])
        .await?
        .is_some_and(|r| r.try_get::<bool>("", "ok").unwrap_or(false)))
}

/// Each agent's weekly review this person can fix ($1 to $4 as in FIXERS).
pub(super) async fn list(tx: &DatabaseTransaction, args: &[sea_orm::Value]) -> Result<Vec<Value>> {
    if !ready(tx).await? {
        return Ok(vec![]);
    }
    json_rows(tx, &format!("{} SELECT jsonb_build_object('agent',f.id,'enabled',t.outcome IS NULL AND t.paused_ms IS NULL,'task',t.id,
          'next_ms',(SELECT min(k.due_ms) FROM relay_task_wakeups k WHERE k.workspace=t.workspace AND k.task=t.id AND k.state='pending' AND k.kind<>'end')) AS data
        FROM relay_review_routines rr JOIN f ON f.id=rr.agent JOIN relay_tasks t ON t.workspace=rr.workspace AND t.id=rr.task WHERE rr.workspace=$1", super::FIXERS),
        args.to_vec()).await
}

impl PgStore {
    /// Weekly review on or off for an agent its owner or an admin looks after.
    pub(crate) async fn set_weekly_review(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        enabled: bool,
    ) -> Result<Value> {
        let tx = self.workspace_tx(w).await?;
        ensure!(
            super::available(&tx).await? && ready(&tx).await?,
            "Weekly reviews need the latest workspace database"
        );
        self.fixer(&tx, user, w, agent).await?;
        let current = query(
            &tx,
            "SELECT t.id,t.paused_ms IS NOT NULL AS paused FROM relay_review_routines rr
              JOIN relay_tasks t ON t.workspace=rr.workspace AND t.id=rr.task
             WHERE rr.workspace=$1 AND rr.agent=$2 AND t.outcome IS NULL FOR UPDATE OF t",
            vec![w.into(), agent.into()],
        )
        .await?;
        if let Some(row) = current {
            let id: String = row.try_get("", "id")?;
            if row.try_get::<bool>("", "paused")? == enabled {
                let name = user
                    .display_name
                    .clone()
                    .unwrap_or_else(|| "a person".into());
                let reason = format!("Weekly review turned off by {name}.");
                set_paused(&tx, w, &id, (!enabled).then_some(reason.as_str())).await?;
                audit(
                    &tx,
                    &user.id,
                    Some(w),
                    if enabled {
                        "task_resumed"
                    } else {
                        "task_paused"
                    },
                    &id,
                )
                .await?;
            }
            tx.commit().await?;
            return Ok(json!({"task": id, "enabled": enabled}));
        }
        ensure!(enabled, "Weekly review is already off");
        // The first review a week from now, then every week on that day and time.
        let zone = super::super::super::super::tasks::changes::workspace_zone(&tx, w).await?;
        let when = query(&tx, "SELECT x.ms,extract(isodow FROM to_timestamp(x.ms/1000.0) AT TIME ZONE $1)::bigint AS day,
              to_char(to_timestamp(x.ms/1000.0) AT TIME ZONE $1,'HH24:MI') AS at
            FROM (SELECT (floor(extract(epoch FROM clock_timestamp())/60)*60000)::bigint+604800000 AS ms) x",
            vec![zone.into()]).await?.context("Clock unavailable")?;
        let first: i64 = when.try_get("", "ms")?;
        let rule = Rule {
            unit: "week".into(),
            days: Some(vec![when.try_get("", "day")?]),
            at: Some(when.try_get("", "at")?),
            ..Rule::default()
        };
        let id = create_repeating(
            &tx,
            w,
            agent,
            &user.id,
            "Weekly review",
            REVIEW,
            &rule,
            first,
        )
        .await?;
        execute(&tx, "INSERT INTO relay_review_routines(workspace,agent,task) VALUES($1,$2,$3) ON CONFLICT(workspace,agent) DO UPDATE SET task=excluded.task",
            vec![w.into(), agent.into(), id.clone().into()]).await?;
        audit(&tx, &user.id, Some(w), "weekly_review_on", &id).await?;
        tx.commit().await?;
        Ok(json!({"task": id, "enabled": true}))
    }
}

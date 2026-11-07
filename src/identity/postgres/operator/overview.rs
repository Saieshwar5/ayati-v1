//! The live overview: key numbers, model traffic for the last hour, alerts and
//! the busiest workspaces. Every alert is worked out from stored facts.
use super::*;

/// Alert thresholds. Small samples are ignored so one failure is not an alert.
const ERROR_RATE_PERCENT: f64 = 5.0;
const MIN_CALLS_FOR_RATE: i64 = 20;
const CEILING_WARNING_PERCENT: f64 = 80.0;
const QUEUE_WAIT_SECONDS: i64 = 120;

impl PgStore {
    pub(crate) async fn operator_overview(&self) -> Result<Value> {
        let now_ms = now() * 1000;
        let (month, day) = month_and_day_start();
        let pool = query(&self.db, include_str!("../pool_metrics.sql"), vec![])
            .await?
            .context("Pool metrics unavailable")?;
        let numbers = query(&self.db, "SELECT
              (SELECT count(*) FROM relay_workspaces w WHERE to_jsonb(w)->>'closed_ms' IS NULL) AS workspaces,
              (SELECT count(DISTINCT workspace) FROM relay_model_usage WHERE at_ms>=$1-7*86400000) AS active,
              (SELECT count(*) FROM relay_users) AS people,
              (SELECT count(*) FROM relay_model_usage WHERE at_ms>=$1-300000) AS calls_5m,
              (SELECT count(*) FROM relay_model_usage WHERE at_ms>=$1-3600000) AS calls_1h,
              (SELECT count(*) FROM relay_model_failures WHERE at_ms>=$1-3600000) AS errors_1h,
              (SELECT count(*) FROM relay_model_usage WHERE at_ms>=$1-3600000 AND attempts>1) AS fell_back_1h",
            vec![now_ms.into()]).await?.context("Overview unavailable")?;
        let (charge_today, cost_today) = self.totals_since(day).await?;
        let (charge_month, cost_month) = self.totals_since(month).await?;
        // Calls per provider in twelve 5-minute buckets, oldest first.
        let start = now_ms - 3_600_000;
        let traffic = json_rows(&self.db, "SELECT jsonb_build_object('provider',p.provider,'buckets',
              (SELECT jsonb_agg(COALESCE(c.n,0) ORDER BY b.i) FROM generate_series(0,11) b(i)
               LEFT JOIN (SELECT ((at_ms-$1)/300000)::int AS i,count(*) AS n FROM relay_model_usage
                 WHERE provider=p.provider AND at_ms>=$1 GROUP BY 1) c ON c.i=b.i)) AS data
            FROM (SELECT DISTINCT provider FROM relay_model_usage WHERE at_ms>=$1) p ORDER BY p.provider",
            vec![start.into()]).await?;
        let busiest = json_rows(&self.db, "SELECT jsonb_build_object('id',w.id,'name',w.name,
              'running',(SELECT count(*) FROM relay_chat_jobs j WHERE j.workspace=w.id AND j.state='running'),
              'queued',(SELECT count(*) FROM relay_chat_jobs j WHERE j.workspace=w.id AND j.state='queued'),
              'calls_5m',a.calls,'charge_today',(SELECT COALESCE(sum(charge_micro),0) FROM relay_model_usage u WHERE u.workspace=w.id AND u.at_ms>=$2)/1000000.0) AS data
            FROM (SELECT workspace,count(*) AS calls FROM relay_model_usage WHERE at_ms>=$1 GROUP BY workspace ORDER BY count(*) DESC LIMIT 8) a
            JOIN relay_workspaces w ON w.id=a.workspace ORDER BY a.calls DESC", vec![(now_ms - 300_000).into(), day.into()]).await?;
        let oldest_queued: i64 = pool.try_get("", "oldest_queued_seconds")?;
        let mut alerts = self.provider_alerts(now_ms).await?;
        alerts.extend(self.spend_alerts(day).await?);
        if oldest_queued > QUEUE_WAIT_SECONDS {
            alerts.push(json!({"level":"warn","kind":"queue","title":"Work is waiting for executors",
                "body":format!("The oldest request has waited {oldest_queued} s for a free slot."),"target":null}));
        }
        let reports: i64 = query(
            &self.db,
            "SELECT count(*) AS n FROM relay_problem_reports WHERE created_ms>=$1",
            vec![(now_ms - 86_400_000).into()],
        )
        .await?
        .context("Missing count")?
        .try_get("", "n")?;
        if reports > 0 {
            alerts.push(json!({"level":"info","kind":"reports","title":format!("{reports} problem report(s) in the last day"),
                "body":"People reported problems from Settings → Help.","target":null}));
        }
        let expiring: i64 = query(&self.db, "SELECT count(*) AS n FROM relay_workspace_codes WHERE used_at IS NULL AND revoked_at IS NULL
            AND expires_at>$1 AND expires_at<$1+7*86400", vec![now().into()]).await?.context("Missing count")?.try_get("", "n")?;
        if expiring > 0 {
            alerts.push(json!({"level":"info","kind":"codes","title":format!("{expiring} unused invite code(s) expire this week"),
                "body":"Send them or revoke them.","target":null}));
        }
        Ok(json!({
            "workspaces": numbers.try_get::<i64>("", "workspaces")?, "active_7d": numbers.try_get::<i64>("", "active")?,
            "people": numbers.try_get::<i64>("", "people")?,
            "calls_per_min": numbers.try_get::<i64>("", "calls_5m")? as f64 / 5.0,
            "calls_1h": numbers.try_get::<i64>("", "calls_1h")?, "errors_1h": numbers.try_get::<i64>("", "errors_1h")?,
            "fell_back_1h": numbers.try_get::<i64>("", "fell_back_1h")?,
            "today": {"charge": charge_today as f64 / 1e6, "cost": cost_today as f64 / 1e6},
            "month": {"charge": charge_month as f64 / 1e6, "cost": cost_month as f64 / 1e6},
            "daily_ceiling": self.platform_limit(settings::DAILY_CEILING).await?.map(|c| c as f64 / 1e6),
            "pool": {"queued": pool.try_get::<i64>("", "queued")?, "oldest_queued_seconds": oldest_queued,
                "ready": pool.try_get::<i64>("", "ready_slots")?, "busy": pool.try_get::<i64>("", "busy_slots")?,
                "executors": pool.try_get::<i64>("", "executors")?},
            "traffic_start_ms": start, "traffic": traffic, "busiest": busiest, "alerts": alerts,
        }))
    }

    async fn provider_alerts(&self, now_ms: i64) -> Result<Vec<Value>> {
        let mut alerts = Vec::new();
        for row in rows(&self.db, "SELECT provider,note FROM relay_model_provider_switches WHERE NOT enabled ORDER BY provider", vec![]).await? {
            let provider: String = row.try_get("", "provider")?;
            alerts.push(json!({"level":"bad","kind":"provider_off","title":format!("{provider} is switched off"),
                "body":format!("No calls reach it; catalog routes use their other providers. Note: {}", row.try_get::<String>("", "note")?),"target":provider}));
        }
        for row in rows(
            &self.db,
            "SELECT p.provider,
              (SELECT count(*) FROM relay_model_usage u WHERE u.provider=p.provider AND u.at_ms>=$1) AS answered,
              (SELECT count(*) FROM relay_model_failures f WHERE f.provider=p.provider AND f.at_ms>=$1) AS errors
            FROM (SELECT provider FROM relay_model_failures WHERE at_ms>=$1 GROUP BY provider) p",
            vec![(now_ms - 900_000).into()],
        )
        .await?
        {
            let (answered, errors): (i64, i64) =
                (row.try_get("", "answered")?, row.try_get("", "errors")?);
            let calls = answered + errors;
            let rate = 100.0 * errors as f64 / calls.max(1) as f64;
            if calls >= MIN_CALLS_FOR_RATE && rate >= ERROR_RATE_PERCENT {
                let provider: String = row.try_get("", "provider")?;
                alerts.push(json!({"level":"warn","kind":"provider_errors","title":format!("{provider} is failing {rate:.1}% of calls"),
                    "body":format!("{errors} of {calls} calls failed in the last 15 minutes."),"target":provider}));
            }
        }
        Ok(alerts)
    }

    /// Workspace budgets past their warning share (schema 89), and the ceiling.
    async fn spend_alerts(&self, day: i64) -> Result<Vec<Value>> {
        let mut alerts = Vec::new();
        let (month, _) = month_and_day_start();
        for row in rows(&self.db, "SELECT w.id,w.name,b.monthly_micro AS budget,b.warn_percent::bigint AS warn,
              (SELECT COALESCE(sum(u.charge_micro),0) FROM relay_model_usage u WHERE u.workspace=w.id AND u.at_ms>=$1)::bigint AS used
            FROM relay_model_budgets b JOIN relay_workspaces w ON w.id=b.workspace WHERE b.agent='' ORDER BY w.name LIMIT 500",
            vec![month.into()]).await? {
            let (used, budget, warn): (i64, i64, i64) = (row.try_get("", "used")?, row.try_get("", "budget")?, row.try_get("", "warn")?);
            let pct = 100.0 * used as f64 / budget.max(1) as f64;
            if pct >= warn as f64 {
                let name: String = row.try_get("", "name")?;
                alerts.push(json!({"level": if pct >= 100.0 { "bad" } else { "warn" }, "kind":"budget",
                    "title":format!("{name} has used {pct:.0}% of its monthly budget"),
                    "body":format!("{} of {}. Its admins set the budget; model calls stop at it.", spend::dollars(used), spend::dollars(budget)),
                    "target":row.try_get::<String>("", "id")?}));
            }
        }
        if let Some(ceiling) = self.platform_limit(settings::DAILY_CEILING).await? {
            let cost = self.cost_since(day).await?;
            if ceiling > 0 && cost as f64 >= ceiling as f64 * CEILING_WARNING_PERCENT / 100.0 {
                alerts.push(json!({"level": if cost >= ceiling { "bad" } else { "warn" }, "kind":"ceiling",
                    "title":"Today's provider cost is near the platform ceiling",
                    "body":format!("{} of {} today.", spend::dollars(cost), spend::dollars(ceiling)),"target":null}));
            }
        }
        Ok(alerts)
    }
}

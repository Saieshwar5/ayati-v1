//! Spend across the platform, from each call's charge (what people pay at the
//! catalog price) and cost (what the provider charges us, at the route's cost),
//! both saved by the gateway (schema 88). Plus the operator limits the gateway
//! enforces before any call: suspension and the platform's daily ceiling.
use super::*;

/// Why the gateway must refuse a workspace's next model call, if it must.
/// Workspace and agent budgets (schema 89) are checked by the gateway separately.
pub(crate) async fn refusal(db: &PgStore, w: &str) -> Result<Option<String>> {
    let suspended: bool = query(
        &db.db,
        "SELECT to_jsonb(w)->>'suspended_ms' IS NOT NULL AS suspended FROM relay_workspaces w WHERE w.id=$1",
        vec![w.into()],
    )
    .await?
    .context("Workspace unavailable")?
    .try_get("", "suspended")?;
    if suspended {
        return Ok(Some("This workspace is suspended by Cuckoo".into()));
    }
    let (_, day) = month_and_day_start();
    if let Some(ceiling) = db.platform_limit(settings::DAILY_CEILING).await?
        && db.cost_since(day).await? >= ceiling
    {
        return Ok(Some(
            "Cuckoo reached today's AI spending limit. Try again after midnight UTC.".into(),
        ));
    }
    Ok(None)
}

pub(crate) fn dollars(micros: i64) -> String {
    format!("${:.2}", micros as f64 / 1_000_000.0)
}

const DAY_MS: i64 = 86_400_000;

impl PgStore {
    /// What providers charged us since `since_ms`, across every workspace.
    pub(crate) async fn cost_since(&self, since_ms: i64) -> Result<i64> {
        Ok(query(
            &self.db,
            "SELECT COALESCE(sum(cost_micro),0)::bigint AS cost FROM relay_model_usage WHERE at_ms>=$1",
            vec![since_ms.into()],
        )
        .await?
        .context("Missing spend")?
        .try_get("", "cost")?)
    }

    /// Charged and cost since `since_ms`, in micro-dollars.
    pub(crate) async fn totals_since(&self, since_ms: i64) -> Result<(i64, i64)> {
        let row = query(
            &self.db,
            "SELECT COALESCE(sum(charge_micro),0)::bigint AS charge, COALESCE(sum(cost_micro),0)::bigint AS cost
            FROM relay_model_usage WHERE at_ms>=$1",
            vec![since_ms.into()],
        )
        .await?
        .context("Missing spend")?;
        Ok((row.try_get("", "charge")?, row.try_get("", "cost")?))
    }

    pub(crate) async fn operator_spend(&self, days: i64) -> Result<Value> {
        let days = days.clamp(1, 90);
        let (month, day) = month_and_day_start();
        let (month_charge, month_cost) = self.totals_since(month).await?;
        let (today_charge, today_cost) = self.totals_since(day).await?;
        let elapsed = ((now() * 1000 - month) as f64 / DAY_MS as f64).max(0.25);
        let month_days = ((month_start(month + 32 * DAY_MS) - month) / DAY_MS) as f64;
        let forecast = |v: i64| v as f64 / elapsed * month_days / 1e6;
        let daily = json_rows(&self.db, "SELECT jsonb_build_object('day_ms',d.day,
              'charge',COALESCE(sum(u.charge_micro),0)/1000000.0,'cost',COALESCE(sum(u.cost_micro),0)/1000000.0,'calls',count(u.id)) AS data
            FROM generate_series($1::bigint,$2::bigint,86400000) AS d(day)
            LEFT JOIN relay_model_usage u ON u.at_ms>=d.day AND u.at_ms<d.day+86400000
            GROUP BY d.day ORDER BY d.day", vec![(day - (days - 1) * DAY_MS).into(), day.into()]).await?;
        let by_model = json_rows(&self.db, "SELECT jsonb_build_object('model',model,'calls',count(*),
              'input_tokens',sum(input_tokens),'output_tokens',sum(output_tokens),
              'charge',COALESCE(sum(charge_micro),0)/1000000.0,'cost',COALESCE(sum(cost_micro),0)/1000000.0,
              'margin',(COALESCE(sum(charge_micro),0)-COALESCE(sum(cost_micro),0))/1000000.0) AS data
            FROM relay_model_usage WHERE at_ms>=$1 GROUP BY model ORDER BY COALESCE(sum(charge_micro),0) DESC, count(*) DESC LIMIT 50",
            vec![month.into()]).await?;
        let by_workspace = json_rows(&self.db, "SELECT jsonb_build_object('id',w.id,'name',w.name,
              'charge',s.charge/1000000.0,'cost',s.cost/1000000.0,'margin',(s.charge-s.cost)/1000000.0,'calls',s.calls,
              'budget',(SELECT b.monthly_micro/1000000.0 FROM relay_model_budgets b WHERE b.workspace=w.id AND b.agent=''),
              'top_agent',(SELECT jsonb_build_object('name',COALESCE(a.name,u.agent),'charge',sum(u.charge_micro)/1000000.0)
                 FROM relay_model_usage u LEFT JOIN relay_shared_agents a ON a.workspace=u.workspace AND a.id=u.agent
                 WHERE u.workspace=w.id AND u.at_ms>=$1 GROUP BY COALESCE(a.name,u.agent) ORDER BY sum(u.charge_micro) DESC LIMIT 1)) AS data
            FROM (SELECT workspace,COALESCE(sum(charge_micro),0) AS charge,COALESCE(sum(cost_micro),0) AS cost,count(*) AS calls
              FROM relay_model_usage WHERE at_ms>=$1 GROUP BY workspace) s
            JOIN relay_workspaces w ON w.id=s.workspace ORDER BY s.charge DESC LIMIT 25", vec![month.into()]).await?;
        let ceiling = self.platform_limit(settings::DAILY_CEILING).await?;
        Ok(json!({
            "month_start_ms": month,
            "month": {"charge": month_charge as f64 / 1e6, "cost": month_cost as f64 / 1e6, "margin": (month_charge - month_cost) as f64 / 1e6},
            "today": {"charge": today_charge as f64 / 1e6, "cost": today_cost as f64 / 1e6},
            "forecast": {"charge": forecast(month_charge), "cost": forecast(month_cost)},
            "daily_ceiling": ceiling.map(|c| c as f64 / 1e6),
            "daily": daily, "by_model": by_model, "by_workspace": by_workspace,
        }))
    }
}

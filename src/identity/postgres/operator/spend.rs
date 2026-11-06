//! Model spend: from token counts and catalog prices at call time, per
//! workspace, model and day; and the limits the gateway enforces.
use super::*;

/// Why the gateway must refuse a workspace's next model call, if it must.
pub(crate) async fn refusal(db: &PgStore, w: &str) -> Result<Option<String>> {
    let row = query(
        &db.db,
        "SELECT to_jsonb(w)->>'suspended_ms' IS NOT NULL AS suspended,
        (to_jsonb(w)->>'spend_cap_micros')::bigint AS cap FROM relay_workspaces w WHERE w.id=$1",
        vec![w.into()],
    )
    .await?
    .context("Workspace unavailable")?;
    if row.try_get::<bool>("", "suspended")? {
        return Ok(Some("This workspace is suspended by Cuckoo".into()));
    }
    let (month, day) = month_and_day_start();
    let cap = match row.try_get::<Option<i64>>("", "cap")? {
        Some(cap) => Some(cap),
        None => db.platform_limit(settings::DEFAULT_CAP).await?,
    };
    if let Some(cap) = cap
        && db.spent_since(Some(w), month).await? >= cap
    {
        return Ok(Some(format!(
            "This workspace reached its monthly AI spending limit of {}. An admin can ask Cuckoo to raise it.",
            dollars(cap)
        )));
    }
    if let Some(ceiling) = db.platform_limit(settings::DAILY_CEILING).await?
        && db.spent_since(None, day).await? >= ceiling
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

impl PgStore {
    /// Micro-dollars spent since `since_ms`, for one workspace or all of them.
    pub(crate) async fn spent_since(&self, w: Option<&str>, since_ms: i64) -> Result<i64> {
        Ok(query(
            &self.db,
            "SELECT COALESCE(sum(cost_micros),0)::bigint AS spent FROM relay_model_usage
            WHERE at_ms>=$2 AND ($1::text IS NULL OR workspace=$1)",
            vec![w.map(str::to_owned).into(), since_ms.into()],
        )
        .await?
        .context("Missing spend")?
        .try_get("", "spent")?)
    }

    pub(crate) async fn operator_spend(&self, days: i64) -> Result<Value> {
        let days = days.clamp(1, 90);
        let (month, day) = month_and_day_start();
        let since = day - (days - 1) * 86_400_000;
        let month_spent = self.spent_since(None, month).await?;
        let today = self.spent_since(None, day).await?;
        let now_ms = now() * 1000;
        let elapsed_days = ((now_ms - month) as f64 / 86_400_000.0).max(0.25);
        let next_month = month_start(month + 32 * 86_400_000);
        let month_days = ((next_month - month) / 86_400_000) as f64;
        let forecast = (month_spent as f64 / elapsed_days * month_days) as i64;
        let daily = json_rows(&self.db, "SELECT jsonb_build_object('day_ms',d.day,'cost',COALESCE(sum(u.cost_micros),0)/1000000.0,'calls',count(u.id)) AS data
            FROM generate_series($1::bigint,$2::bigint,86400000) AS d(day)
            LEFT JOIN relay_model_usage u ON u.at_ms>=d.day AND u.at_ms<d.day+86400000
            GROUP BY d.day ORDER BY d.day", vec![since.into(), day.into()]).await?;
        let by_model = json_rows(&self.db, "SELECT jsonb_build_object('provider',provider,'model',model,'calls',count(*),
              'input_tokens',sum(input_tokens),'output_tokens',sum(output_tokens),'cost',COALESCE(sum(cost_micros),0)/1000000.0,
              'unpriced_calls',count(*) FILTER (WHERE cost_micros IS NULL AND outcome='ok')) AS data
            FROM relay_model_usage WHERE at_ms>=$1 GROUP BY provider,model ORDER BY COALESCE(sum(cost_micros),0) DESC, count(*) DESC LIMIT 50",
            vec![month.into()]).await?;
        let default_cap = self.platform_limit(settings::DEFAULT_CAP).await?;
        let by_workspace = json_rows(&self.db, "SELECT jsonb_build_object('id',w.id,'name',w.name,'cost',s.cost/1000000.0,'calls',s.calls,
              'cap',COALESCE((to_jsonb(w)->>'spend_cap_micros')::bigint,$2::bigint)/1000000.0,
              'top_agent',(SELECT jsonb_build_object('name',COALESCE(a.name,u.agent),'cost',sum(u.cost_micros)/1000000.0)
                 FROM relay_model_usage u LEFT JOIN relay_shared_agents a ON a.workspace=u.workspace AND a.id=u.agent
                 WHERE u.workspace=w.id AND u.at_ms>=$1 AND u.cost_micros IS NOT NULL GROUP BY COALESCE(a.name,u.agent) ORDER BY sum(u.cost_micros) DESC LIMIT 1)) AS data
            FROM (SELECT workspace,COALESCE(sum(cost_micros),0) AS cost,count(*) AS calls FROM relay_model_usage WHERE at_ms>=$1 GROUP BY workspace) s
            JOIN relay_workspaces w ON w.id=s.workspace ORDER BY s.cost DESC LIMIT 25", vec![month.into(), default_cap.into()]).await?;
        let ceiling = self.platform_limit(settings::DAILY_CEILING).await?;
        Ok(json!({
            "month_start_ms": month, "month": month_spent as f64 / 1e6, "today": today as f64 / 1e6,
            "forecast": forecast as f64 / 1e6, "daily_ceiling": ceiling.map(|c| c as f64 / 1e6),
            "default_cap": default_cap.map(|c| c as f64 / 1e6),
            "daily": daily, "by_model": by_model, "by_workspace": by_workspace,
        }))
    }
}

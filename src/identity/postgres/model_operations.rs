//! The operator's view of models (MG4): for Cuckoo's platform administrators only.
//! Provider health (calls, fallbacks, failures by status, speed) and money (what
//! people are charged against what providers charge us) across every workspace.
use super::*;
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};
use serde_json::{Value, json};

/// Schema 90: how long each call took, and every call a provider failed.
pub(crate) struct ModelOperations;
impl MigrationName for ModelOperations {
    fn name(&self) -> &str {
        "m20261106_000090_model_operations"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for ModelOperations {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(
                "ALTER TABLE relay_model_usage ADD COLUMN latency_ms INTEGER CHECK(latency_ms>=0);
                 CREATE INDEX relay_model_usage_recent ON relay_model_usage(at_ms DESC);
                 CREATE TABLE relay_model_failures (
                  id BIGSERIAL PRIMARY KEY,
                  workspace TEXT NOT NULL REFERENCES relay_workspaces(id),
                  provider TEXT NOT NULL CHECK(char_length(provider) BETWEEN 1 AND 64),
                  model TEXT NOT NULL CHECK(char_length(model) BETWEEN 1 AND 200),
                  -- The provider's HTTP status, or 0 when it couldn't be reached.
                  status SMALLINT NOT NULL CHECK(status BETWEEN 0 AND 599),
                  at_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint);
                 CREATE INDEX relay_model_failures_recent ON relay_model_failures(at_ms DESC);",
            )
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Model operations require a forward migration".into(),
        ))
    }
}

const PROVIDERS: &str = "SELECT u.provider AS key, count(*)::bigint AS calls,
  count(*) FILTER (WHERE u.attempts>1)::bigint AS after_fallback,
  COALESCE(sum(u.input_tokens+u.output_tokens),0)::bigint AS tokens,
  COALESCE(sum(u.charge_micro),0)::bigint AS charge, COALESCE(sum(u.cost_micro),0)::bigint AS cost,
  COALESCE(avg(u.latency_ms),0)::bigint AS latency_avg,
  COALESCE(percentile_disc(0.95) WITHIN GROUP (ORDER BY u.latency_ms),0)::bigint AS latency_p95
 FROM relay_model_usage u WHERE u.at_ms>=$1 AND u.assignment<>'library' GROUP BY 1 ORDER BY calls DESC LIMIT 50";
const MODELS: &str = "SELECT u.model AS key, count(*)::bigint AS calls,
  count(*) FILTER (WHERE u.attempts>1)::bigint AS after_fallback,
  COALESCE(sum(u.input_tokens+u.output_tokens),0)::bigint AS tokens,
  COALESCE(sum(u.charge_micro),0)::bigint AS charge, COALESCE(sum(u.cost_micro),0)::bigint AS cost,
  COALESCE(avg(u.latency_ms),0)::bigint AS latency_avg,
  COALESCE(percentile_disc(0.95) WITHIN GROUP (ORDER BY u.latency_ms),0)::bigint AS latency_p95
 FROM relay_model_usage u WHERE u.at_ms>=$1 AND u.assignment<>'library' GROUP BY 1 ORDER BY calls DESC LIMIT 200";
const FAILURES: &str =
    "SELECT provider, status::bigint AS status, count(*)::bigint AS count, max(at_ms) AS last_ms
 FROM relay_model_failures WHERE at_ms>=$1 GROUP BY 1,2 ORDER BY 3 DESC LIMIT 200";

fn line(row: &QueryResult) -> Result<Value> {
    let charge: i64 = row.try_get("", "charge")?;
    let cost: i64 = row.try_get("", "cost")?;
    Ok(json!({
        "key": row.try_get::<String>("", "key")?,
        "calls": row.try_get::<i64>("", "calls")?,
        "after_fallback": row.try_get::<i64>("", "after_fallback")?,
        "tokens": row.try_get::<i64>("", "tokens")?,
        "charge_usd": charge as f64 / 1e6,
        "cost_usd": cost as f64 / 1e6,
        "margin_usd": (charge - cost) as f64 / 1e6,
        "latency_avg_ms": row.try_get::<i64>("", "latency_avg")?,
        "latency_p95_ms": row.try_get::<i64>("", "latency_p95")?,
    }))
}

impl PgStore {
    /// A provider failed a call: kept so operators see errors across controllers.
    pub(crate) async fn record_model_failure(
        &self,
        w: &str,
        provider: &str,
        model: &str,
        status: u16,
    ) -> Result<()> {
        execute(
            &self.db,
            "INSERT INTO relay_model_failures(workspace,provider,model,status) VALUES($1,$2,$3,$4)",
            vec![
                w.into(),
                provider.into(),
                model.into(),
                i16::try_from(status.min(599)).unwrap_or(0).into(),
            ],
        )
        .await?;
        Ok(())
    }

    /// Every workspace's model traffic since `since_ms`. Callers check the person is a
    /// platform administrator.
    pub(crate) async fn model_operations(&self, since_ms: i64) -> Result<Value> {
        let tx = self.db.begin().await?;
        let mut report = json!({"since_ms": since_ms});
        for (name, sql) in [("providers", PROVIDERS), ("models", MODELS)] {
            let mut list = vec![];
            for row in rows(&tx, sql, vec![since_ms.into()]).await? {
                list.push(line(&row)?);
            }
            report[name] = json!(list);
        }
        let mut failures = vec![];
        for row in rows(&tx, FAILURES, vec![since_ms.into()]).await? {
            failures.push(json!({"provider": row.try_get::<String>("", "provider")?, "status": row.try_get::<i64>("", "status")?,
                "count": row.try_get::<i64>("", "count")?, "last_ms": row.try_get::<i64>("", "last_ms")?}));
        }
        report["failures"] = json!(failures);
        tx.commit().await?;
        Ok(report)
    }
}

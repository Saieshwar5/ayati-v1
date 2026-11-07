//! Token use per model request through the gateway (core-agent CA3), for Activity,
//! usage and budgets.
use super::*;
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};
use serde_json::json;

/// Schema 68: model usage through the gateway.
pub(crate) struct ModelUsage;
impl MigrationName for ModelUsage {
    fn name(&self) -> &str {
        "m20261105_000068_model_usage"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for ModelUsage {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("model_usage.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Model usage requires a forward migration".into(),
        ))
    }
}

/// One model call: who made it, which catalog model (or plain model) it was for, and
/// the provider route that served it.
pub(crate) struct ModelCall<'a> {
    pub workspace: &'a str,
    pub assignment: &'a str,
    pub agent: &'a str,
    pub job: Option<&'a str>,
    pub provider: &'a str,
    /// The catalog model people chose, or the plain model outside the catalog.
    pub model: &'a str,
    /// The provider's own name for it.
    pub provider_model: &'a str,
    /// Routes tried, counting the one that answered.
    pub attempts: usize,
    pub input_tokens: i64,
    pub output_tokens: i64,
    /// The person the work was for: a request's requester or a task's actor.
    pub person: Option<&'a str>,
    /// What the call is charged at the catalog price, in millionths of a dollar.
    pub charge_micro: i64,
    /// What the provider charges us for it, at the route's cost.
    pub cost_micro: i64,
    /// From sending the call to the end of the answer.
    pub latency_ms: Option<i64>,
}

/// Millionths of a dollar for these tokens at a price in dollars per million tokens:
/// conveniently, tokens × price.
pub(crate) fn micro_dollars(
    input: i64,
    output: i64,
    price: &crate::identity::models::catalog::Price,
) -> i64 {
    let value = input.max(0) as f64 * price.input + output.max(0) as f64 * price.output;
    if value.is_finite() {
        value.round() as i64
    } else {
        0
    }
}

/// Schema 87: which provider route served each call, and how many were tried.
pub(crate) struct ModelRoutes;
impl MigrationName for ModelRoutes {
    fn name(&self) -> &str {
        "m20261106_000087_model_routes"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for ModelRoutes {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(
                "ALTER TABLE relay_model_usage ADD COLUMN provider_model TEXT CHECK(char_length(provider_model)<=200);
                 ALTER TABLE relay_model_usage ADD COLUMN attempts SMALLINT NOT NULL DEFAULT 1 CHECK(attempts BETWEEN 1 AND 9);",
            )
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Model routes require a forward migration".into(),
        ))
    }
}

/// Schema 88: who each call was for, and what it was charged and cost (MG2).
pub(crate) struct ModelCosts;
impl MigrationName for ModelCosts {
    fn name(&self) -> &str {
        "m20261106_000088_model_costs"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for ModelCosts {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(
                "ALTER TABLE relay_model_usage ADD COLUMN person TEXT;
                 ALTER TABLE relay_model_usage ADD COLUMN charge_micro BIGINT NOT NULL DEFAULT 0 CHECK(charge_micro>=0);
                 ALTER TABLE relay_model_usage ADD COLUMN cost_micro BIGINT NOT NULL DEFAULT 0 CHECK(cost_micro>=0);
                 CREATE INDEX relay_model_usage_time ON relay_model_usage(workspace,at_ms DESC);",
            )
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Model costs require a forward migration".into(),
        ))
    }
}

impl PgStore {
    pub(crate) async fn record_model_usage(&self, call: &ModelCall<'_>) -> Result<()> {
        execute(&self.db, "INSERT INTO relay_model_usage(workspace,assignment,agent,job,provider,model,input_tokens,output_tokens,provider_model,attempts,person,charge_micro,cost_micro,latency_ms) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)",
            vec![call.workspace.into(), call.assignment.into(), call.agent.into(), call.job.map(str::to_owned).into(), call.provider.into(), call.model.into(), Ord::max(call.input_tokens, 0).into(), Ord::max(call.output_tokens, 0).into(), call.provider_model.into(), i16::try_from(call.attempts.min(9)).unwrap_or(1).into(),
                call.person.map(str::to_owned).into(), Ord::max(call.charge_micro, 0).into(), Ord::max(call.cost_micro, 0).into(), call.latency_ms.map(|ms| i32::try_from(ms.max(0)).unwrap_or(i32::MAX)).into()]).await?;
        Ok(())
    }
    /// Model use for one request; callers have already checked the person may see it.
    pub(crate) async fn job_model_usage(&self, w: &str, job: &str) -> Result<serde_json::Value> {
        let row = query(&self.db, "SELECT count(*)::bigint AS calls, COALESCE(sum(input_tokens),0)::bigint AS input, COALESCE(sum(output_tokens),0)::bigint AS output, COALESCE(sum(charge_micro),0)::bigint AS charge, string_agg(DISTINCT model, ', ') AS models FROM relay_model_usage WHERE workspace=$1 AND job=$2", vec![w.into(), job.into()]).await?.unwrap();
        Ok(
            json!({"calls":row.try_get::<i64>("","calls")?,"input_tokens":row.try_get::<i64>("","input")?,"output_tokens":row.try_get::<i64>("","output")?,
                "charge_usd":row.try_get::<i64>("","charge")? as f64 / 1e6,"models":row.try_get::<Option<String>>("","models")?}),
        )
    }
}

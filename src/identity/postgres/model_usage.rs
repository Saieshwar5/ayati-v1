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

/// One model call as the gateway saw it.
pub(crate) struct ModelCall<'a> {
    pub workspace: &'a str,
    pub assignment: &'a str,
    pub agent: &'a str,
    pub job: Option<&'a str>,
    pub provider: &'a str,
    pub model: &'a str,
    /// The model the agent asked for, when a route served it with another.
    pub requested_model: Option<&'a str>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub latency_ms: Option<i64>,
    pub ok: bool,
    /// A backup served this call after the main target failed.
    pub fell_back: bool,
}

impl PgStore {
    /// A successful call outside the gateway (Library models), priced from the catalog.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn record_model_usage(
        &self,
        w: &str,
        assignment: &str,
        agent: &str,
        job: Option<&str>,
        provider: &str,
        model: &str,
        input_tokens: i64,
        output_tokens: i64,
    ) -> Result<()> {
        self.record_model_call(&ModelCall {
            workspace: w,
            assignment,
            agent,
            job,
            provider,
            model,
            requested_model: None,
            input_tokens,
            output_tokens,
            latency_ms: None,
            ok: true,
            fell_back: false,
        })
        .await
    }

    /// One model call, priced at the catalog's current price (schema 87). Failed
    /// calls are recorded with no tokens so the console can show error rates.
    pub(crate) async fn record_model_call(&self, call: &ModelCall<'_>) -> Result<()> {
        execute(&self.db, "INSERT INTO relay_model_usage(workspace,assignment,agent,job,provider,model,input_tokens,output_tokens,
              cost_micros,latency_ms,outcome,fell_back,requested_model)
            SELECT $1,$2,$3,$4,$5,$6,$7,$8,
              CASE WHEN $10 THEN (SELECT (($7::numeric*p.input_micros_per_mtok + $8::numeric*p.output_micros_per_mtok)/1000000)::bigint
                FROM relay_platform_models p WHERE p.provider=$5 AND p.model=$6) END,
              $9,CASE WHEN $10 THEN 'ok' ELSE 'error' END,$11,$12",
            vec![call.workspace.into(), call.assignment.into(), call.agent.into(), call.job.map(str::to_owned).into(),
                call.provider.into(), call.model.into(), Ord::max(call.input_tokens, 0).into(), Ord::max(call.output_tokens, 0).into(),
                call.latency_ms.map(|ms| ms.min(i32::MAX as i64) as i32).into(), call.ok.into(), call.fell_back.into(),
                call.requested_model.map(str::to_owned).into()]).await?;
        Ok(())
    }
    /// Model use for one request; callers have already checked the person may see it.
    pub(crate) async fn job_model_usage(&self, w: &str, job: &str) -> Result<serde_json::Value> {
        let row = query(&self.db, "SELECT count(*)::bigint AS calls, COALESCE(sum(input_tokens),0)::bigint AS input, COALESCE(sum(output_tokens),0)::bigint AS output, string_agg(DISTINCT model, ', ') AS models FROM relay_model_usage WHERE workspace=$1 AND job=$2 AND outcome='ok'", vec![w.into(), job.into()]).await?.unwrap();
        Ok(
            json!({"calls":row.try_get::<i64>("","calls")?,"input_tokens":row.try_get::<i64>("","input")?,"output_tokens":row.try_get::<i64>("","output")?,"models":row.try_get::<Option<String>>("","models")?}),
        )
    }
}

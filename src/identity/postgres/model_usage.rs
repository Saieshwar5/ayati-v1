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

impl PgStore {
    pub(crate) async fn record_model_usage(&self, call: &ModelCall<'_>) -> Result<()> {
        execute(&self.db, "INSERT INTO relay_model_usage(workspace,assignment,agent,job,provider,model,input_tokens,output_tokens,provider_model,attempts) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
            vec![call.workspace.into(), call.assignment.into(), call.agent.into(), call.job.map(str::to_owned).into(), call.provider.into(), call.model.into(), Ord::max(call.input_tokens, 0).into(), Ord::max(call.output_tokens, 0).into(), call.provider_model.into(), i16::try_from(call.attempts.min(9)).unwrap_or(1).into()]).await?;
        Ok(())
    }
    /// Model use for one request; callers have already checked the person may see it.
    pub(crate) async fn job_model_usage(&self, w: &str, job: &str) -> Result<serde_json::Value> {
        let row = query(&self.db, "SELECT count(*)::bigint AS calls, COALESCE(sum(input_tokens),0)::bigint AS input, COALESCE(sum(output_tokens),0)::bigint AS output, string_agg(DISTINCT model, ', ') AS models FROM relay_model_usage WHERE workspace=$1 AND job=$2", vec![w.into(), job.into()]).await?.unwrap();
        Ok(
            json!({"calls":row.try_get::<i64>("","calls")?,"input_tokens":row.try_get::<i64>("","input")?,"output_tokens":row.try_get::<i64>("","output")?,"models":row.try_get::<Option<String>>("","models")?}),
        )
    }
}

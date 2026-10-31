use sea_orm_migration::prelude::*;
pub(super) struct AgentAudience;
impl MigrationName for AgentAudience {
    fn name(&self) -> &str {
        "m20261031_000045_agent_audience"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for AgentAudience {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("agent_audience.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Agent audiences require a forward migration".into(),
        ))
    }
}

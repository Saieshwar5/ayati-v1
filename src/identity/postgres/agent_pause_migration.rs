use sea_orm_migration::prelude::*;
pub(crate) struct AgentPause;
impl MigrationName for AgentPause {
    fn name(&self) -> &str {
        "m20261101_000051_agent_pause"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for AgentPause {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("agent_pause.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Pausing agents requires a forward migration".into(),
        ))
    }
}

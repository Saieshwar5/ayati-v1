use sea_orm_migration::prelude::*;
pub(crate) struct AgentUpdates;
impl MigrationName for AgentUpdates {
    fn name(&self) -> &str {
        "m20261101_000052_agent_updates"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for AgentUpdates {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("agent_updates.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Agent updates require a forward migration".into(),
        ))
    }
}

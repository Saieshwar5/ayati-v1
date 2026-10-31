use sea_orm_migration::prelude::*;
pub(super) struct AgentInstructions;
impl MigrationName for AgentInstructions {
    fn name(&self) -> &str {
        "m20261031_000046_agent_instructions"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for AgentInstructions {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("agent_instructions.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Agent instructions require a forward migration".into(),
        ))
    }
}

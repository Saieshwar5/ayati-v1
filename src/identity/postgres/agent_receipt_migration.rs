use sea_orm_migration::prelude::*;
pub(super) struct AgentReceipts;
impl MigrationName for AgentReceipts {
    fn name(&self) -> &str {
        "m20261030_000039_agent_receipts"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for AgentReceipts {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("agent_receipts.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Agent receipts require a forward migration".into(),
        ))
    }
}

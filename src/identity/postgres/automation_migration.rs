use sea_orm_migration::prelude::*;
pub(super) struct Automations;
impl MigrationName for Automations {
    fn name(&self) -> &str {
        "m20261026_000027_shared_agent_schedules"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for Automations {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(include_str!("automations.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Schedule occurrence receipts require a forward migration".into(),
        ))
    }
}

use sea_orm_migration::prelude::*;
pub(super) struct Events;
impl MigrationName for Events {
    fn name(&self) -> &str {
        "m20261026_000028_shared_agent_events"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for Events {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(include_str!("events.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Event receipt tombstones require a forward migration".into(),
        ))
    }
}

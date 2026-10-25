use sea_orm_migration::prelude::*;

pub(super) struct DurableWaits;
impl MigrationName for DurableWaits {
    fn name(&self) -> &str {
        "m20261026_000022_durable_approval_waits"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for DurableWaits {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(include_str!("durable_waits.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Task checkpoints require a forward migration".into(),
        ))
    }
}

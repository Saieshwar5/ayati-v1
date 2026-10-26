use sea_orm_migration::prelude::*;
pub(super) struct Rehearsals;
impl MigrationName for Rehearsals {
    fn name(&self) -> &str {
        "m20261026_000026_agent_rehearsals"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for Rehearsals {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(include_str!("rehearsals.sql"))
            .await?;
        manager
            .get_connection()
            .execute_unprepared(include_str!("rehearsal_invalidation.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Saved sample-task receipts require a forward migration".into(),
        ))
    }
}

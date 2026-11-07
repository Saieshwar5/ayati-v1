use sea_orm_migration::prelude::*;
pub(super) struct Handoffs;
impl MigrationName for Handoffs {
    fn name(&self) -> &str {
        "m20261026_000029_channel_agent_handoffs"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for Handoffs {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("handoffs.sql"))
            .await?;
        m.get_connection()
            .execute_unprepared(include_str!("handoff_revocation.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Handoff receipts require a forward migration".into(),
        ))
    }
}
/// Schema 100 (D246): a handoff asked from the main timeline is posted there,
/// so its receipt has no thread.
pub(super) struct VisibleHandoffs;
impl MigrationName for VisibleHandoffs {
    fn name(&self) -> &str {
        "m20261107_000100_visible_handoffs"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for VisibleHandoffs {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("visible_handoffs.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Visible handoffs require a forward migration".into(),
        ))
    }
}

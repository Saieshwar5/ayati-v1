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

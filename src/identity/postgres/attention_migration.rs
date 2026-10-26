use sea_orm_migration::prelude::*;
pub(super) struct Attention;
impl MigrationName for Attention {
    fn name(&self) -> &str {
        "m20261026_000025_workspace_attention"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for Attention {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(include_str!("attention_receipts.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Personal attention receipts require a forward migration".into(),
        ))
    }
}

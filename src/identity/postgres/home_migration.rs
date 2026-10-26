use sea_orm_migration::prelude::*;
pub(super) struct HomePanels;
impl MigrationName for HomePanels {
    fn name(&self) -> &str {
        "m20261026_000030_workspace_home_panels"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for HomePanels {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("home_panels.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Durable Home panels need a forward migration".into(),
        ))
    }
}

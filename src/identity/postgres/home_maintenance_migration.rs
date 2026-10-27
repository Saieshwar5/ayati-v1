use sea_orm_migration::prelude::*;
pub(super) struct HomeMaintenance;
impl MigrationName for HomeMaintenance {
    fn name(&self) -> &str {
        "m20261027_000033_home_panel_maintenance"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for HomeMaintenance {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("home_maintenance.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Home maintenance requires a forward migration".into(),
        ))
    }
}

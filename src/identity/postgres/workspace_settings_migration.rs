use sea_orm_migration::prelude::*;
pub(crate) struct WorkspaceSettings;
impl MigrationName for WorkspaceSettings {
    fn name(&self) -> &str {
        "m20261031_000049_workspace_settings"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for WorkspaceSettings {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("workspace_settings.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Workspace settings require a forward migration".into(),
        ))
    }
}

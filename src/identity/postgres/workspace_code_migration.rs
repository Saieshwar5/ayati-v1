use sea_orm_migration::prelude::*;
pub(super) struct WorkspaceCodes;
impl MigrationName for WorkspaceCodes {
    fn name(&self) -> &str {
        "m20261030_000041_workspace_codes"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for WorkspaceCodes {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("workspace_codes.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Workspace codes require a forward migration".into(),
        ))
    }
}

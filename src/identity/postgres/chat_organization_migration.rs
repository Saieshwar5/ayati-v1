use sea_orm_migration::prelude::*;
pub(super) struct ChatOrganization;
impl MigrationName for ChatOrganization {
    fn name(&self) -> &str {
        "m20261026_000024_chat_organization"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for ChatOrganization {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(include_str!("chat_organization.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Chat history and preferences require a forward migration".into(),
        ))
    }
}

use sea_orm_migration::prelude::*;
pub(super) struct MessageActions;
impl MigrationName for MessageActions {
    fn name(&self) -> &str {
        "m20261030_000037_message_actions"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for MessageActions {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("message_actions.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Message actions require a forward migration".into(),
        ))
    }
}

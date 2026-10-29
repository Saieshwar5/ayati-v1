use sea_orm_migration::prelude::*;
pub(super) struct ChatNotifications;
impl MigrationName for ChatNotifications {
    fn name(&self) -> &str {
        "m20261030_000038_chat_notifications"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for ChatNotifications {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("chat_notifications.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Chat notifications require a forward migration".into(),
        ))
    }
}

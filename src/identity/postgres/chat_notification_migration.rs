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

/// Schema 140: notification choices reach the person's other devices.
pub(super) struct NotifyLevelChanges;
impl MigrationName for NotifyLevelChanges {
    fn name(&self) -> &str {
        "m20261120_000140_notify_level_changes"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for NotifyLevelChanges {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("chat_notify_changes.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Notification level changes require a forward migration".into(),
        ))
    }
}

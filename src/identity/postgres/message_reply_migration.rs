use sea_orm_migration::prelude::*;
pub(super) struct MessageReplies;
impl MigrationName for MessageReplies {
    fn name(&self) -> &str {
        "m20261030_000040_message_replies"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for MessageReplies {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("message_replies.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Message replies require a forward migration".into(),
        ))
    }
}

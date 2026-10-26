use sea_orm_migration::prelude::*;
pub(super) struct ConversationContext;
impl MigrationName for ConversationContext {
    fn name(&self) -> &str {
        "m20261026_000031_conversation_context"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for ConversationContext {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("conversation_context.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Saved context requires a forward migration".into(),
        ))
    }
}

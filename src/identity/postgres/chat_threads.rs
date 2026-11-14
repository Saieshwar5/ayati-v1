//! Threads beside the conversation (schema 115): each root reports how many
//! replies it has and when the last came, from the server rather than the
//! loaded page, and a reply can also be sent to the conversation.
use sea_orm_migration::prelude::*;

pub(in crate::identity::postgres) struct ChatThreads;
impl MigrationName for ChatThreads {
    fn name(&self) -> &str {
        "m20261114_000115_chat_threads"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for ChatThreads {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("chat_threads.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Thread summaries require a forward migration".into(),
        ))
    }
}

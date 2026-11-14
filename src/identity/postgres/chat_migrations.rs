//! Chat migrations of 2026-11-14. Threads beside the conversation (schema
//! 115): each root reports how many replies it has and when the last came,
//! from the server rather than the loaded page, and a reply can also be sent
//! to the conversation. Schema 116 adds the level that hides a conversation.
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

/// Schema 116: "Nothing, and hide it", a fourth level for one conversation.
pub(in crate::identity::postgres) struct ChatHideLevel;
impl MigrationName for ChatHideLevel {
    fn name(&self) -> &str {
        "m20261114_000116_chat_hide_level"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for ChatHideLevel {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("chat_hide_level.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "The hide level requires a forward migration".into(),
        ))
    }
}

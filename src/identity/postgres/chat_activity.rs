//! When each conversation last had activity, for the Chat list's order (schema 96).
//!
//! A conversation's activity is the newest of: its last sent message or finished
//! reply, when a reply finished after it was created, when the conversation was
//! created, and when this person was added to it. An agent's progress notes,
//! edits, reactions and receipts are not activity. Live message payloads carry
//! `at` and `activity_at`, so the client moves a conversation up without reading
//! the list again.
use super::*;
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};

pub(crate) struct ChatActivity;
impl MigrationName for ChatActivity {
    fn name(&self) -> &str {
        "m20261107_000096_chat_activity"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for ChatActivity {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("chat_activity.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Chat activity requires a forward migration".into(),
        ))
    }
}

/// Conversation `c`'s activity for person `$2`, in seconds like `at`. The last
/// message reads backwards on the (workspace, chat, sequence) index, the latest
/// finish on its partial index.
pub(super) const ACTIVITY: &str = "greatest(c.created_ms/1000,
 coalesce((SELECT m.at FROM relay_chat_messages m WHERE m.workspace=c.workspace AND m.chat=c.id AND m.update_of IS NULL AND m.state IN ('sent','complete') ORDER BY m.sequence DESC LIMIT 1),0),
 coalesce((SELECT max(m.activity_at) FROM relay_chat_messages m WHERE m.workspace=c.workspace AND m.chat=c.id AND m.activity_at IS NOT NULL),0),
 coalesce((SELECT p.added_ms/1000 FROM relay_chat_people p WHERE p.workspace=c.workspace AND p.chat=c.id AND p.user_id=$2),0))";

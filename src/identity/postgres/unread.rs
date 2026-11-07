//! Unread counts per conversation, and when each person joined (schema 94).
//!
//! A message is unread for a person when someone else sent it, it is sent or
//! complete, the person has no seen receipt for it, and the person could see it
//! when it arrived. The conversation must be one Chat lists as active: archived
//! channels and a schedule's work room never count. The message must not be
//! older than the person's access: for a public channel, joining the workspace;
//! for a private conversation, the later of joining the workspace and being
//! added to it. A person who joins today starts with nothing unread.
use super::*;
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};
use serde_json::json;

#[cfg(test)]
#[path = "unread_tests.rs"]
mod tests;

pub(crate) struct UnreadFromJoining;
impl MigrationName for UnreadFromJoining {
    fn name(&self) -> &str {
        "m20261107_000094_unread_from_joining"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for UnreadFromJoining {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        // now() is stable, so PostgreSQL stores the value once for existing rows
        // (metadata only, no table rewrite): everyone already in a workspace or
        // conversation starts at this migration. New rows get the time of their
        // insert, so creating a workspace, accepting an invitation, a link or a
        // code, and being added to a conversation all record a join time.
        m.get_connection()
            .execute_unprepared(
                "ALTER TABLE relay_members ADD COLUMN joined_ms BIGINT NOT NULL
                   DEFAULT (extract(epoch FROM now())*1000)::bigint;
                 ALTER TABLE relay_chat_people ADD COLUMN joined_ms BIGINT NOT NULL
                   DEFAULT (extract(epoch FROM now())*1000)::bigint;",
            )
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Unread join times require a forward migration".into(),
        ))
    }
}

impl PgStore {
    pub async fn unread(&self, user: &User, w: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        role(&tx, w, &user.id).await?;
        // The same conversations Chat lists (chats_in), minus archived ones.
        let hidden = super::work_rooms::hidden(&tx).await?;
        // `m.at` is in seconds and join times in milliseconds. A message from the
        // second someone joined counts, rather than risk missing one.
        let sql = format!(
            "SELECT jsonb_build_object('chat',m.chat,'count',count(*),'mentions',count(*) FILTER (WHERE {for_me})) AS data
             FROM relay_chats c
             JOIN relay_members me ON me.workspace=c.workspace AND me.user_id=$2
             LEFT JOIN relay_chat_people p ON p.workspace=c.workspace AND p.chat=c.id AND p.user_id=$2
             JOIN relay_chat_messages m ON m.workspace=c.workspace AND m.chat=c.id
             WHERE c.workspace=$1 AND NOT c.archived{hidden}
             AND (c.visibility='public' OR p.user_id IS NOT NULL)
             AND m.author<>$2 AND m.state IN ('sent','complete')
             AND m.at>=greatest(me.joined_ms,CASE WHEN c.visibility='public' THEN 0 ELSE p.joined_ms END)/1000
             AND NOT EXISTS(SELECT 1 FROM relay_message_receipts r WHERE r.workspace=m.workspace AND r.message=m.id AND r.user_id=$2 AND r.seen_at IS NOT NULL)
             GROUP BY m.chat",
            for_me = super::chat_notifications::FOR_ME
        );
        let items = json_rows(&tx, &sql, vec![w.into(), user.id.clone().into()]).await?;
        tx.commit().await?;
        Ok(json!({"unread":items}))
    }
}

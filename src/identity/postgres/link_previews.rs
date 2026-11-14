//! The cache behind link previews (`chat/link_preview.rs`), schema 118: one
//! row per web address, shared because a preview is what the public page
//! says about itself. A preview is only ever returned for a message the
//! reader may read, and only for people's messages: agents' replies have
//! per-viewer views of their own.
use super::*;
use crate::identity::chat::link_preview::{Preview, first_link};

/// A preview is read again after a day; a page that gave none, after an hour.
const FRESH: i64 = 24 * 3600;
const RETRY: i64 = 3600;

impl PgStore {
    /// The first web link of a person's sent message that `user` can read.
    pub(crate) async fn message_link(
        &self,
        user: &User,
        w: &str,
        chat: &str,
        message: &str,
    ) -> Result<Option<String>> {
        let tx = self.read_tx(w).await?;
        super::chat::access(&tx, user, w, chat).await?;
        let content = query(
            &tx,
            "SELECT content FROM relay_chat_messages WHERE workspace=$1 AND chat=$2 AND id=$3 AND kind='user' AND state='sent'",
            vec![w.into(), chat.into(), message.into()],
        )
        .await?
        .map(|row| row.try_get::<String>("", "content"))
        .transpose()?;
        tx.commit().await?;
        Ok(content.as_deref().and_then(first_link))
    }

    /// A fresh cached answer: `Some(null)` for a page known to have no preview.
    pub(crate) async fn cached_preview(&self, url: &str) -> Result<Option<Value>> {
        let row = query(
            &self.db,
            "SELECT preview FROM relay_link_previews WHERE url=$1 AND fetched > $2 - CASE WHEN preview IS NULL THEN $4 ELSE $3 END",
            vec![url.into(), now().into(), FRESH.into(), RETRY.into()],
        )
        .await?;
        Ok(row
            .map(|row| row.try_get::<Option<Value>>("", "preview"))
            .transpose()?
            .map(|preview| preview.unwrap_or(Value::Null)))
    }

    pub(crate) async fn save_preview(&self, url: &str, preview: Option<&Preview>) -> Result<()> {
        execute(
            &self.db,
            "INSERT INTO relay_link_previews(url,preview,fetched) VALUES($1,$2,$3)
             ON CONFLICT(url) DO UPDATE SET preview=EXCLUDED.preview,fetched=EXCLUDED.fetched",
            vec![
                url.into(),
                preview.map(|p| serde_json::json!(p)).into(),
                now().into(),
            ],
        )
        .await?;
        Ok(())
    }
}

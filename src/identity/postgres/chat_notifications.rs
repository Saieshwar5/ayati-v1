//! Personal notification levels per conversation, and who a message is for.
//! Levels are presentation preferences; they never change access.
use super::*;
use serde_json::json;

pub const LEVELS: [&str; 3] = ["all", "mentions", "none"];

/// Matches unread messages that mention the person or answer their request.
pub(super) const FOR_ME: &str =
    "(strpos(m.content,'](mention:user:'||$2||')')>0 OR (m.kind='agent' AND m.requester=$2))";

impl PgStore {
    pub async fn chat_notify_levels(&self, user: &User, w: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        role(&tx, w, &user.id).await?;
        let rows = rows(
            &tx,
            "SELECT chat,level FROM relay_chat_notify WHERE workspace=$1 AND user_id=$2",
            vec![w.into(), user.id.clone().into()],
        )
        .await?;
        tx.commit().await?;
        let mut levels = serde_json::Map::new();
        for row in rows {
            levels.insert(
                row.try_get("", "chat")?,
                json!(row.try_get::<String>("", "level")?),
            );
        }
        Ok(json!({ "levels": levels }))
    }

    /// Sets the caller's level for one conversation they can read.
    pub async fn set_chat_notify(
        &self,
        user: &User,
        w: &str,
        chat: &str,
        level: &str,
    ) -> Result<()> {
        ensure!(LEVELS.contains(&level), "Choose all, mentions or none");
        let tx = self.workspace_tx(w).await?;
        super::chat::access(&tx, user, w, chat).await?;
        execute(
            &tx,
            "INSERT INTO relay_chat_notify(workspace,user_id,chat,level) VALUES($1,$2,$3,$4)
             ON CONFLICT(workspace,user_id,chat) DO UPDATE SET level=EXCLUDED.level",
            vec![w.into(), user.id.clone().into(), chat.into(), level.into()],
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
}

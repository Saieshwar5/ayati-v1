//! Chat files: the uploaded objects, what was read from them and how agents
//! find them. Every path checks the message that shared a file.
use super::*;
use crate::identity::collaboration::Object;

#[path = "agent_files.rs"]
pub(super) mod agent_files;
#[path = "object_contents.rs"]
pub(crate) mod contents;
#[path = "conversation_files.rs"]
mod conversation_files;

impl PgStore {
    /// Saves an uploaded file. With `transcribe`, audio also queues its
    /// transcript in the same transaction (schema 95). Other files queue their
    /// reading for agents (schema 100).
    pub(crate) async fn save_object(
        &self,
        user: &User,
        w: &str,
        chat: &str,
        obj: &Object,
        transcribe: bool,
    ) -> Result<()> {
        let tx = self.workspace_tx(w).await?;
        super::chat::access(&tx, user, w, chat).await?;
        super::channel_lifecycle::accepting(&tx, w, chat).await?;
        execute(&tx,"INSERT INTO relay_objects(workspace,chat,id,uploader,name,mime,size,checksum,created) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)",vec![w.into(),chat.into(),obj.id.clone().into(),user.id.clone().into(),obj.name.clone().into(),obj.mime.clone().into(),obj.size.into(),obj.checksum.clone().into(),now().into()]).await?;
        if transcribe && obj.mime.starts_with("audio/") {
            super::transcripts::queue_in(&tx, w, &obj.id).await?;
        }
        contents::queue_in(&tx, w, &obj.id, &obj.mime).await?;
        tx.commit().await?;
        Ok(())
    }
    pub(crate) async fn object(&self, user: &User, w: &str, id: &str) -> Result<Object> {
        let tx = self.read_tx(w).await?;
        let row = query(
            &tx,
            "SELECT to_jsonb(o) AS data FROM relay_objects o WHERE workspace=$1 AND id=$2",
            vec![w.into(), id.into()],
        )
        .await?
        .context("File unavailable")?;
        let data: Value = row.try_get("", "data")?;
        super::chat::access(
            &tx,
            user,
            w,
            data["chat"].as_str().context("Missing conversation")?,
        )
        .await?;
        let published=query(&tx,"SELECT id FROM relay_chat_messages WHERE workspace=$1 AND chat=$2 AND attachments @> $3::jsonb LIMIT 1",vec![w.into(),data["chat"].as_str().unwrap().into(),serde_json::json!([id]).into()]).await?.is_some();
        ensure!(
            data["uploader"] == user.id || published,
            "Attachment is not published"
        );
        tx.commit().await?;
        Ok(serde_json::from_value(data)?)
    }
}

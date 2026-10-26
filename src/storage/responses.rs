use super::*;
use rusqlite::OptionalExtension;
impl Store {
    pub(crate) fn response_message(&self, id: &str) -> Result<ChatMessage> {
        self.connection()?.query_row("SELECT id,task_id,role,content,state,created_at FROM messages WHERE id=?1 AND role='assistant'",[id],|r|Ok(ChatMessage{id:r.get(0)?,task_id:r.get(1)?,role:r.get(2)?,content:r.get(3)?,state:r.get(4)?,created_at:r.get(5)?})).optional()?.context("Saved response unavailable")
    }
}

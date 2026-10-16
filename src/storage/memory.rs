use super::{Store, work::RunClaim};
use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};

// Conversation scope is deliberately narrower than agent scope. There is no
// shared/customer memory routing until authenticated identities and grants exist.
#[derive(Clone, Debug)]
pub struct MemoryScope(pub(crate) String);
impl MemoryScope {
    pub fn local() -> Self {
        Self(super::runtime::LOCAL_CONVERSATION.into())
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Remember {
    pub key: String,
    pub content: String,
    pub source_id: String,
    pub quote: String,
    pub replaces: Option<String>,
}
#[derive(Serialize, Debug)]
pub struct Memory {
    pub id: String,
    pub key: String,
    pub content: String,
    pub source_id: String,
    pub quote: String,
    pub source_kind: String,
    pub source_date: String,
    pub created_at: String,
    pub supersedes: Option<String>,
}
fn key(value: &str) -> Result<String> {
    let value = value.trim().to_ascii_lowercase();
    ensure!(
        !value.is_empty()
            && value.len() <= 80
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b)),
        "Memory key must be 1–80 ASCII letters, digits, dots, underscores or hyphens"
    );
    Ok(value)
}
impl Store {
    pub fn memory_scope(&self, claim: &RunClaim) -> Result<MemoryScope> {
        ensure!(self.claim_active(claim)?, "Memory run is no longer active");
        Ok(MemoryScope(self.connection()?.query_row(
            "SELECT conversation_id FROM run_scopes WHERE task_id=?1",
            [&claim.task_id],
            |r| r.get(0),
        )?))
    }
    pub fn memory_source(&self, claim: &RunClaim) -> Result<serde_json::Value> {
        let scope = self.memory_scope(claim)?;
        let mut conn = self.connection()?;
        let tx = conn.transaction()?;
        tx.execute("INSERT OR IGNORE INTO memory_sources(id,conversation_id,kind,text,created_at) SELECT m.id,s.conversation_id,'owner_message',m.content,m.created_at FROM messages m JOIN run_scopes s ON s.task_id=m.task_id WHERE m.task_id=?1 AND m.role='user' AND s.conversation_id=?2 AND NOT EXISTS(SELECT 1 FROM event_receipts WHERE task_id=m.task_id) AND NOT EXISTS(SELECT 1 FROM schedule_runs WHERE task_id=m.task_id)", params![claim.task_id,scope.0])?;
        let source = tx.query_row("SELECT id,text,created_at FROM memory_sources WHERE id IN (SELECT id FROM messages WHERE task_id=?1 AND role='user') AND conversation_id=?2", params![claim.task_id,scope.0], |r| Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"text":r.get::<_,String>(1)?,"date":r.get::<_,String>(2)?})))?;
        tx.commit()?;
        Ok(source)
    }
    pub fn owner_memory_note(
        &self,
        scope: &MemoryScope,
        key: &str,
        content: &str,
        replaces: Option<String>,
    ) -> Result<Memory> {
        let id = uuid::Uuid::new_v4().to_string();
        let mut conn = self.connection()?;
        let tx = conn.transaction()?;
        ensure!(
            !content.trim().is_empty() && content.len() <= 2000,
            "Memory content must contain 1–2000 bytes"
        );
        tx.execute("INSERT INTO memory_sources(id,conversation_id,kind,text) VALUES(?1,?2,'owner_note',?3)", params![id,scope.0,content])?;
        let result = Self::remember_in(
            &tx,
            scope,
            &Remember {
                key: key.into(),
                content: content.into(),
                source_id: id,
                quote: content.into(),
                replaces,
            },
        )?;
        tx.commit()?;
        Ok(result)
    }
    pub fn remember(&self, scope: &MemoryScope, input: &Remember) -> Result<Memory> {
        let mut conn = self.connection()?;
        let tx = conn.transaction()?;
        let result = Self::remember_in(&tx, scope, input)?;
        tx.commit()?;
        Ok(result)
    }
    fn remember_in(tx: &Transaction<'_>, scope: &MemoryScope, input: &Remember) -> Result<Memory> {
        let key = key(&input.key)?;
        ensure!(
            !input.content.trim().is_empty()
                && input.content.len() <= 2000
                && !input.quote.trim().is_empty()
                && input.quote.len() <= 2000,
            "Memory content and quote must contain 1–2000 bytes"
        );
        let source: Option<String> = tx.query_row("SELECT text FROM memory_sources WHERE id=?1 AND conversation_id=?2 AND NOT EXISTS(SELECT 1 FROM memory_source_exclusions WHERE source_id=?1)", params![input.source_id,scope.0], |r| r.get(0)).optional()?;
        ensure!(
            source.is_some_and(|s| s.contains(&input.quote)),
            "Source is unavailable in this scope, forgotten, or does not contain the exact quote"
        );
        ensure!(!tx.query_row("SELECT EXISTS(SELECT 1 FROM memory_exclusions WHERE conversation_id=?1 AND key=?2)", params![scope.0,key], |r|r.get::<_,bool>(0))?, "This memory key was forgotten; automatic relearning is blocked");
        let previous: Option<String> = tx
            .query_row(
                "SELECT id FROM memories WHERE conversation_id=?1 AND key=?2 AND state='active'",
                params![scope.0, key],
                |r| r.get(0),
            )
            .optional()?;
        ensure!(
            previous == input.replaces,
            "Memory changed or already exists; supply the current id as replaces"
        );
        if let Some(id) = &previous {
            tx.execute("UPDATE memories SET state='superseded' WHERE id=?1", [id])?;
            Self::invalidate_memory_context(tx, scope)?;
        }
        let id = uuid::Uuid::new_v4().to_string();
        tx.execute("INSERT INTO memories(id,conversation_id,key,content,source_id,quote,supersedes,state) VALUES(?1,?2,?3,?4,?5,?6,?7,'active')",params![id,scope.0,key,input.content,input.source_id,input.quote,previous])?;
        Self::read_memory(tx, &id)
    }
    pub fn forget_memory(&self, scope: &MemoryScope, id: &str) -> Result<()> {
        let mut conn = self.connection()?;
        let tx = conn.transaction()?;
        let (key,source):(String,String)=tx.query_row("SELECT key,source_id FROM memories WHERE conversation_id=?1 AND id=?2 AND state='active'", params![scope.0,id], |r|Ok((r.get(0)?,r.get(1)?))).context("Active memory not found in this scope")?;
        // Suppress the entire evidence source and all facts derived from it.
        // This prevents relearning a paraphrase under a different key.
        tx.execute("INSERT OR IGNORE INTO memory_exclusions SELECT conversation_id,key FROM memories WHERE conversation_id=?1 AND (key=?2 OR source_id=?3)", params![scope.0,key,source])?;
        tx.execute("INSERT OR IGNORE INTO memory_source_exclusions SELECT DISTINCT source_id FROM memories WHERE conversation_id=?1 AND (key=?2 OR source_id=?3)", params![scope.0,key,source])?;
        tx.execute("UPDATE memories SET state='forgotten' WHERE conversation_id=?1 AND (key IN (SELECT key FROM memory_exclusions WHERE conversation_id=?1) OR source_id IN (SELECT source_id FROM memory_source_exclusions))",[&scope.0])?;
        Self::invalidate_memory_context(&tx, scope)?;
        tx.commit()?;
        Ok(())
    }
    fn invalidate_memory_context(tx: &Transaction<'_>, scope: &MemoryScope) -> Result<()> {
        tx.execute(
            "DELETE FROM runtime_sessions WHERE conversation_id=?1",
            [&scope.0],
        )?;
        Ok(())
    }
    pub fn search_memory(&self, scope: &MemoryScope, query: &str) -> Result<Vec<Memory>> {
        ensure!(query.len() <= 256, "Memory query exceeds 256 bytes");
        let terms: Vec<_> = query
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty())
            .take(12)
            .map(|s| format!("\"{s}\""))
            .collect();
        let conn = self.connection()?;
        let ids: Vec<String> = if terms.is_empty() {
            let mut stmt=conn.prepare("SELECT id FROM memories WHERE conversation_id=?1 AND state='active' ORDER BY rowid DESC LIMIT 8")?;
            stmt.query_map([&scope.0], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?
        } else {
            let mut stmt=conn.prepare("SELECT m.id FROM memory_fts JOIN memories m ON m.rowid=memory_fts.rowid WHERE memory_fts MATCH ?1 AND m.conversation_id=?2 AND m.state='active' ORDER BY bm25(memory_fts),m.rowid DESC LIMIT 8")?;
            stmt.query_map(params![terms.join(" OR "), scope.0], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?
        };
        let mut results = Vec::new();
        let mut bytes = 0;
        for id in ids {
            let memory = Self::read_memory(&conn, &id)?;
            bytes += serde_json::to_vec(&memory)?.len();
            if bytes > 8000 {
                break;
            }
            results.push(memory);
        }
        Ok(results)
    }
    fn read_memory(conn: &rusqlite::Connection, id: &str) -> Result<Memory> {
        Ok(conn.query_row("SELECT m.id,m.key,m.content,m.source_id,m.quote,s.kind,s.created_at,m.created_at,m.supersedes FROM memories m JOIN memory_sources s ON s.id=m.source_id WHERE m.id=?1",[id],|r|Ok(Memory{id:r.get(0)?,key:r.get(1)?,content:r.get(2)?,source_id:r.get(3)?,quote:r.get(4)?,source_kind:r.get(5)?,source_date:r.get(6)?,created_at:r.get(7)?,supersedes:r.get(8)?}))?)
    }
}

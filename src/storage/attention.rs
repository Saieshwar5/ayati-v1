//! Durable owner attention, independent of the recent conversation window.
use super::{ChatMessage, Store, Task, runtime::PendingPermission};
use anyhow::{Result, ensure};
use rusqlite::params;
use serde::Serialize;

#[derive(Serialize)]
pub struct AttentionTask {
    #[serde(flatten)]
    pub task: Task,
    pub request: String,
}

#[derive(Serialize)]
pub struct Attention {
    pub active: Vec<AttentionTask>,
    pub recent: Vec<AttentionTask>,
    pub outcomes: Vec<AttentionTask>,
    pub outcome_count: usize,
    pub permissions: Vec<PendingPermission>,
}

impl Store {
    pub fn task_messages(&self, id: &str) -> Result<Vec<ChatMessage>> {
        let connection = self.connection()?;
        let mut stmt = connection.prepare("SELECT id,task_id,role,content,state,created_at FROM messages WHERE task_id=?1 ORDER BY rowid")?;
        Ok(stmt
            .query_map([id], |r| {
                Ok(ChatMessage {
                    id: r.get(0)?,
                    task_id: r.get(1)?,
                    role: r.get(2)?,
                    content: r.get(3)?,
                    state: r.get(4)?,
                    created_at: r.get(5)?,
                })
            })?
            .collect::<Result<_, _>>()?)
    }

    pub fn attention(&self) -> Result<Attention> {
        let mut connection = self.connection()?;
        let tx = connection.transaction()?;
        let read_tasks = |condition: &str| -> Result<Vec<AttentionTask>> {
            let mut stmt = tx.prepare(&format!(
                "SELECT t.id,t.status,t.detail,t.created_at,COALESCE((SELECT substr(content,1,240) FROM messages WHERE task_id=t.id AND role='user' ORDER BY rowid LIMIT 1),'') FROM tasks t WHERE {condition} ORDER BY t.rowid"
            ))?;
            Ok(stmt
                .query_map([], |r| {
                    Ok(AttentionTask {
                        task: Self::read_task(r)?,
                        request: r.get(4)?,
                    })
                })?
                .collect::<Result<_, _>>()?)
        };
        let active = read_tasks("t.status IN ('queued','running')")?;
        let recent = read_tasks(
            "t.id IN (SELECT id FROM tasks WHERE status IN ('completed','failed','interrupted','stopped') ORDER BY rowid DESC LIMIT 6)",
        )?;
        let outstanding = "t.status IN ('failed','interrupted') AND NOT EXISTS(SELECT 1 FROM attention_reviews a WHERE a.task_id=t.id)";
        let outcome_count = tx.query_row(
            &format!("SELECT COUNT(*) FROM tasks t WHERE {outstanding}"),
            [],
            |r| r.get::<_, i64>(0),
        )?;
        // Bound the inbox, without losing older unresolved outcomes behind recent work.
        let outcomes = read_tasks(&format!(
            "t.id IN (SELECT t.id FROM tasks t WHERE {outstanding} ORDER BY t.rowid LIMIT 20)"
        ))?;
        let permissions = {
            let mut stmt = tx.prepare("SELECT p.id,p.task_id,p.title,p.action FROM permissions p JOIN tasks t ON t.id=p.task_id WHERE p.decision='pending' AND t.status='running' AND (p.run_token IS NULL OR EXISTS(SELECT 1 FROM run_claims c WHERE c.task_id=p.task_id AND c.token=p.run_token)) ORDER BY p.rowid")?;
            let rows = stmt.query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get::<_, String>(3)?))
            })?;
            rows.map(|row| {
                let (id, task_id, title, action) = row?;
                Ok(PendingPermission {
                    id,
                    task_id,
                    title,
                    action: serde_json::from_str(&action)?,
                })
            })
            .collect::<Result<Vec<_>>>()?
        };
        tx.commit()?;
        Ok(Attention {
            active,
            recent,
            outcomes,
            outcome_count: outcome_count as usize,
            permissions,
        })
    }

    pub fn review_outcome(&self, id: &str) -> Result<()> {
        let mut connection = self.connection()?;
        let tx = connection.transaction()?;
        let eligible: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM tasks WHERE id=?1 AND status IN ('failed','interrupted'))",
            [id],
            |r| r.get(0),
        )?;
        ensure!(
            eligible,
            "Only failed or interrupted requests can be marked reviewed"
        );
        tx.execute(
            "INSERT OR IGNORE INTO attention_reviews(task_id) VALUES(?1)",
            params![id],
        )?;
        tx.commit()?;
        Ok(())
    }
}

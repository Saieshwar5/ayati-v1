//! Private per-run journal for one Goose session: the accepted request, its
//! claim, streamed reply, tool observations and approvals. Never shared state.
pub mod actions;
mod claim_updates;
pub mod runtime;
pub mod work;
use anyhow::{Context, Result, ensure};
use rusqlite::Connection;
use serde::Serialize;
use std::{path::Path, sync::Mutex};

#[derive(Clone, Serialize)]
pub struct ChatMessage {
    pub id: String,
    pub task_id: String,
    pub role: String,
    pub content: String,
    pub state: String,
    pub created_at: String,
}

#[derive(Clone, Serialize)]
pub struct Task {
    pub id: String,
    pub status: String,
    pub detail: String,
    pub created_at: String,
}

pub struct Journal {
    connection: Mutex<Connection>,
}

impl Journal {
    /// Open a fresh journal for one run. Existing files are refused: a run
    /// never resumes another run's state.
    pub fn open(path: &Path) -> Result<Self> {
        ensure!(
            !path.exists(),
            "A run journal must start empty; {} already exists",
            path.display()
        );
        let connection = Connection::open(path).context("Could not create the run journal")?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch(include_str!("journal.sql"))?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    fn connection(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| anyhow::anyhow!("Database lock unavailable"))
    }

    pub fn messages(&self) -> Result<Vec<ChatMessage>> {
        let connection = self.connection()?;
        let mut stmt = connection.prepare("SELECT id,task_id,role,content,state,created_at FROM (SELECT rowid,* FROM messages ORDER BY rowid DESC LIMIT 200) ORDER BY rowid")?;
        Ok(stmt
            .query_map([], |r| {
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

    pub fn task(&self, id: &str) -> Result<Option<Task>> {
        use rusqlite::OptionalExtension;
        Ok(self
            .connection()?
            .query_row(
                "SELECT id,status,detail,created_at FROM tasks WHERE id=?1",
                [id],
                Self::read_task,
            )
            .optional()?)
    }

    fn read_task(r: &rusqlite::Row<'_>) -> rusqlite::Result<Task> {
        Ok(Task {
            id: r.get(0)?,
            status: r.get(1)?,
            detail: r.get(2)?,
            created_at: r.get(3)?,
        })
    }
}

#[cfg(test)]
mod tests;

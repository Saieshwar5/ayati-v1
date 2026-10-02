use crate::model::Message as ModelMessage;
use anyhow::{Context, Result};
use rusqlite::{Connection, params};
use serde::Serialize;
use std::{path::Path, sync::Mutex};
use uuid::Uuid;

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

#[derive(Clone, Serialize)]
pub struct Artifact {
    pub id: String,
    pub task_id: String,
    pub name: String,
    pub kind: String,
    pub size: u64,
    #[serde(skip)]
    pub stored_name: String,
    pub created_at: String,
}

pub struct Store {
    connection: Mutex<Connection>,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let connection = Connection::open(path).context("Could not open Ayati database")?;
        let version: i64 = connection.pragma_query_value(None, "user_version", |r| r.get(0))?;
        anyhow::ensure!(
            version <= 1,
            "Database is newer than this Ayati version; do not downgrade"
        );
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch(include_str!("schema.sql"))?;
        // Never replay uncertain side effects automatically in this first increment.
        connection.execute("UPDATE tasks SET status='interrupted', detail='Ayati restarted; send a message to continue.' WHERE status IN ('queued','running')", [])?;
        connection.execute(
            "UPDATE messages SET state='interrupted' WHERE state='streaming'",
            [],
        )?;
        connection.execute(
            "UPDATE tool_attempts SET status='interrupted' WHERE status='running'",
            [],
        )?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    fn connection(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| anyhow::anyhow!("Database lock unavailable"))
    }

    pub fn enqueue(&self, text: &str) -> Result<Task> {
        let id = Uuid::new_v4().to_string();
        let mut connection = self.connection()?;
        let tx = connection.transaction()?;
        tx.execute(
            "INSERT INTO tasks (id,status,context) VALUES (?1,'queued','[]')",
            [&id],
        )?;
        tx.execute("INSERT INTO messages (id,task_id,role,content,state) VALUES (?1,?2,'user',?3,'complete')",
            params![Uuid::new_v4().to_string(), id, text])?;
        tx.commit()?;
        drop(connection);
        self.task(&id)?.context("Task was not saved")
    }

    pub fn user_text(&self, task_id: &str) -> Result<String> {
        Ok(self.connection()?.query_row(
            "SELECT content FROM messages WHERE task_id=?1 AND role='user'",
            [task_id],
            |r| r.get(0),
        )?)
    }

    pub fn add_reply(&self, task_id: &str) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        self.connection()?.execute("INSERT INTO messages (id,task_id,role,content,state) VALUES (?1,?2,'assistant','','streaming')", params![id, task_id])?;
        Ok(id)
    }

    pub fn save_reply(&self, id: &str, text: &str, state: &str) -> Result<()> {
        self.connection()?.execute(
            "UPDATE messages SET content=?2,state=?3 WHERE id=?1",
            params![id, text, state],
        )?;
        Ok(())
    }

    pub fn set_task(&self, id: &str, status: &str, detail: &str) -> Result<()> {
        self.connection()?.execute(
            "UPDATE tasks SET status=?2,detail=?3 WHERE id=?1",
            params![id, status, detail],
        )?;
        Ok(())
    }

    pub fn save_context(&self, id: &str, messages: &[ModelMessage]) -> Result<()> {
        self.connection()?.execute(
            "UPDATE tasks SET context=?2 WHERE id=?1",
            params![id, serde_json::to_string(messages)?],
        )?;
        Ok(())
    }

    pub fn recent_context(&self, max_bytes: usize) -> Result<Vec<ModelMessage>> {
        let connection = self.connection()?;
        let mut stmt = connection.prepare(
            "SELECT context FROM tasks WHERE status IN ('completed','failed','stopped','interrupted') AND context!='[]' ORDER BY rowid DESC LIMIT 12",
        )?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        let mut segments = vec![];
        let mut size = 0;
        for row in rows {
            let text = row?;
            if size + text.len() > max_bytes {
                break;
            }
            size += text.len();
            segments.push(serde_json::from_str::<Vec<ModelMessage>>(&text)?);
        }
        Ok(segments.into_iter().rev().flatten().collect())
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

    pub fn tasks(&self) -> Result<Vec<Task>> {
        let connection = self.connection()?;
        let mut stmt = connection.prepare(
            "SELECT id,status,detail,created_at FROM tasks ORDER BY rowid DESC LIMIT 100",
        )?;
        Ok(stmt
            .query_map([], Self::read_task)?
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

    pub fn pending_count(&self) -> Result<usize> {
        Ok(self.connection()?.query_row(
            "SELECT COUNT(*) FROM tasks WHERE status IN ('queued','running')",
            [],
            |r| r.get::<_, i64>(0),
        )? as usize)
    }

    pub fn tool_started(
        &self,
        task: &str,
        call_id: &str,
        name: &str,
        arguments: &str,
    ) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        self.connection()?.execute("INSERT INTO tool_attempts (id,task_id,call_id,name,arguments,status) VALUES (?1,?2,?3,?4,?5,'running')", params![id, task, call_id, name, arguments])?;
        Ok(id)
    }

    pub fn tool_finished(&self, id: &str, status: &str, result: &str) -> Result<()> {
        self.connection()?.execute(
            "UPDATE tool_attempts SET status=?2,result=?3 WHERE id=?1",
            params![id, status, result],
        )?;
        Ok(())
    }

    pub fn add_artifact(
        &self,
        task: &str,
        name: &str,
        kind: &str,
        size: u64,
        stored: &str,
    ) -> Result<Artifact> {
        let id = Uuid::new_v4().to_string();
        let size = i64::try_from(size).context("Artifact size is too large")?;
        self.connection()?.execute("INSERT INTO artifacts (id,task_id,name,kind,size,stored_name) VALUES (?1,?2,?3,?4,?5,?6)", params![id, task, name, kind, size, stored])?;
        self.artifact(&id)?.context("Artifact was not saved")
    }

    pub fn artifacts(&self) -> Result<Vec<Artifact>> {
        let connection = self.connection()?;
        let mut stmt = connection.prepare("SELECT id,task_id,name,kind,size,stored_name,created_at FROM artifacts ORDER BY rowid DESC LIMIT 200")?;
        Ok(stmt
            .query_map([], Self::read_artifact)?
            .collect::<Result<_, _>>()?)
    }

    pub fn artifact(&self, id: &str) -> Result<Option<Artifact>> {
        use rusqlite::OptionalExtension;
        Ok(self.connection()?.query_row("SELECT id,task_id,name,kind,size,stored_name,created_at FROM artifacts WHERE id=?1", [id], Self::read_artifact).optional()?)
    }

    fn read_artifact(r: &rusqlite::Row<'_>) -> rusqlite::Result<Artifact> {
        Ok(Artifact {
            id: r.get(0)?,
            task_id: r.get(1)?,
            name: r.get(2)?,
            kind: r.get(3)?,
            size: r.get::<_, i64>(4)? as u64,
            stored_name: r.get(5)?,
            created_at: r.get(6)?,
        })
    }
}

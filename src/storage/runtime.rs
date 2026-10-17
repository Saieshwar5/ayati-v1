use super::Store;
use anyhow::{Result, ensure};
use rusqlite::{OptionalExtension, params};
use serde::Serialize;

pub const LOCAL_CONVERSATION: &str = "local-conversation";

#[derive(Serialize)]
pub struct PendingPermission {
    pub id: String,
    pub task_id: String,
    pub title: String,
    pub action: serde_json::Value,
}

impl Store {
    pub fn clear_runtime_session(&self) -> Result<()> {
        self.connection()?
            .execute("DELETE FROM runtime_sessions", [])?;
        Ok(())
    }
    /// Adding the trusted workspace extension starts fresh context once. Other
    /// provider/model/tool mismatches continue to fail closed.
    pub fn workspace_runtime_session(&self, settings: &str) -> Result<Option<String>> {
        let mut connection = self.connection()?;
        let tx = connection.transaction()?;
        let previous: Option<String> = tx
            .query_row(
                "SELECT settings FROM runtime_sessions WHERE conversation_id=?1",
                [LOCAL_CONVERSATION],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(previous) = previous {
            let previous: serde_json::Value = serde_json::from_str(&previous)?;
            let mut effective: serde_json::Value = serde_json::from_str(settings)?;
            let added_workspace = previous.get("workspace_adapter").is_none()
                && effective.get("workspace_adapter") == Some(&serde_json::json!(1));
            let changed_authority = previous
                .get("authority_revision")
                .and_then(|v| v.as_i64())
                .unwrap_or(0)
                < effective
                    .get("authority_revision")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
            if added_workspace {
                effective
                    .as_object_mut()
                    .unwrap()
                    .remove("workspace_adapter");
            }
            let mut comparable = previous;
            if changed_authority {
                effective
                    .as_object_mut()
                    .unwrap()
                    .remove("authority_revision");
                comparable
                    .as_object_mut()
                    .unwrap()
                    .remove("authority_revision");
            }
            if (added_workspace || changed_authority) && effective == comparable {
                tx.execute(
                    "DELETE FROM runtime_sessions WHERE conversation_id=?1",
                    [LOCAL_CONVERSATION],
                )?;
            }
        }
        tx.commit()?;
        drop(connection);
        self.runtime_session(settings)
    }
    pub fn cancel_all_browser_permissions(&self) -> Result<()> {
        self.connection()?.execute("UPDATE permissions SET decision='cancelled' WHERE decision='pending' AND action LIKE '%ayati_browser__%'", [])?;
        Ok(())
    }
    pub(super) fn migrate_runtime(connection: &mut rusqlite::Connection) -> Result<()> {
        let tx = connection.transaction()?;
        tx.execute_batch(include_str!("runtime.sql"))?;
        tx.commit()?;
        connection.execute(
            "UPDATE permissions SET decision='cancelled' WHERE decision='pending'",
            [],
        )?;
        Ok(())
    }

    pub fn runtime_session(&self, settings: &str) -> Result<Option<String>> {
        let saved: Option<(String, String)> = self
            .connection()?
            .query_row(
                "SELECT session_id,settings FROM runtime_sessions WHERE conversation_id=?1",
                [LOCAL_CONVERSATION],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((id, previous)) = saved {
            ensure!(
                previous == settings,
                "Saved Goose settings differ. Restore the prior provider/model/tool settings or use a fresh AYATI_DATA_DIR for this proof; existing data was preserved."
            );
            Ok(Some(id))
        } else {
            Ok(None)
        }
    }

    pub fn save_runtime_session(&self, session: &str, settings: &str) -> Result<()> {
        self.connection()?.execute(
            "INSERT INTO runtime_sessions(conversation_id,session_id,runtime_version,settings) VALUES (?1,?2,'1.54.0',?3) ON CONFLICT(conversation_id) DO UPDATE SET session_id=excluded.session_id,settings=excluded.settings",
            params![LOCAL_CONVERSATION, session, settings],
        )?;
        Ok(())
    }

    pub fn add_permission(
        &self,
        task: &str,
        title: &str,
        action: &serde_json::Value,
    ) -> Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let changed = self.connection()?.execute(
            "INSERT INTO permissions(id,task_id,title,action) SELECT ?1,?2,?3,?4 WHERE EXISTS(SELECT 1 FROM tasks WHERE id=?2 AND status='running')",
            params![id, task, title, serde_json::to_string(action)?],
        )?;
        ensure!(changed == 1, "This request is no longer running");
        Ok(id)
    }

    pub fn set_running_detail(&self, task: &str, detail: &str) -> Result<()> {
        self.connection()?.execute(
            "UPDATE tasks SET detail=?2 WHERE id=?1 AND status='running'",
            params![task, detail],
        )?;
        Ok(())
    }

    pub fn finish_reply(&self, reply: &str, status: &str) -> Result<()> {
        self.connection()?.execute(
            "UPDATE messages SET state=?2 WHERE id=?1",
            params![reply, status],
        )?;
        Ok(())
    }

    pub fn permissions(&self) -> Result<Vec<PendingPermission>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare("SELECT id,task_id,title,action FROM permissions WHERE decision='pending' ORDER BY rowid")?;
        let rows = statement.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
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
        .collect()
    }

    pub fn permission_decision(&self, id: &str) -> Result<String> {
        Ok(self.connection()?.query_row(
            "SELECT decision FROM permissions WHERE id=?1",
            [id],
            |r| r.get(0),
        )?)
    }

    pub fn decide_permission(&self, id: &str, allow: bool) -> Result<()> {
        let mut connection = self.connection()?;
        let tx = connection.transaction()?;
        let changed = tx.execute(
            "UPDATE permissions SET decision=?2 WHERE id=?1 AND decision='pending' AND EXISTS(SELECT 1 FROM tasks WHERE tasks.id=permissions.task_id AND tasks.status='running') AND (run_token IS NULL OR EXISTS(SELECT 1 FROM run_claims c WHERE c.task_id=permissions.task_id AND c.token=permissions.run_token))",
            params![id, if allow { "allowed" } else { "denied" }],
        )?;
        ensure!(
            changed == 1,
            "This action is no longer waiting for a decision"
        );
        tx.execute(
            "UPDATE tool_attempts SET status=?2 WHERE permission_id=?1 AND status='pending'",
            params![id, if allow { "authorized" } else { "denied" }],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn cancel_permissions(&self, task: &str) -> Result<()> {
        self.connection()?.execute(
            "UPDATE permissions SET decision='cancelled' WHERE task_id=?1 AND decision='pending'",
            [task],
        )?;
        Ok(())
    }
}

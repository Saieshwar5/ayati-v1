//! Trusted local-owner services. Provider credentials never enter agent VMs.
pub mod app_document;
pub mod app_records;
pub mod apps;
pub mod composio;
mod composio_catalog;
pub(crate) mod composio_mcp;
mod connections;
mod consumer_connections;
pub mod event_sources;
pub mod home_preferences;
pub mod panel_presentation;
pub mod panel_schedules;
pub mod panel_sections;
pub mod panel_templates;
#[cfg(test)]
mod panel_templates_tests;
mod panel_validation;
pub mod panels;
pub mod triggers;
pub mod webhook;
use anyhow::{Context, Result, ensure};
pub use connections::{Connection, Grant};
use rusqlite::Connection as Database;
use std::{path::Path, sync::Mutex};

pub struct Platform {
    database: Mutex<Database>,
    pub gate: tokio::sync::RwLock<()>,
    pub client: Option<composio::Composio>,
    pub user_id: String,
}
pub struct ExecutionRun<'a> {
    pub agent: &'a str,
    pub store: &'a crate::storage::Store,
    pub claim: &'a crate::storage::work::RunClaim,
    pub cancel: &'a tokio_util::sync::CancellationToken,
}
impl Platform {
    pub fn authority_revision(&self, agent: &str) -> Result<i64> {
        use rusqlite::OptionalExtension;
        Ok(self
            .database()?
            .query_row(
                "SELECT revision FROM agent_authority WHERE agent_id=?1",
                [agent],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0))
    }
    pub(super) fn bump_authority_in(db: &Database, agent: &str) -> Result<()> {
        db.execute("INSERT INTO agent_authority VALUES(?1,1) ON CONFLICT(agent_id) DO UPDATE SET revision=revision+1",[agent])?;
        Ok(())
    }
    pub fn open(path: &Path, client: Option<composio::Composio>) -> Result<Self> {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(path)?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
        let database = Database::open(path.join("platform.sqlite"))?;
        database.busy_timeout(std::time::Duration::from_secs(5))?;
        database.pragma_update(None, "synchronous", "FULL")?;
        database.execute_batch(include_str!("schema.sql"))?;
        database.execute(
            "INSERT OR IGNORE INTO identity VALUES(1,?1)",
            [format!("ayati-{}", uuid::Uuid::new_v4())],
        )?;
        let user_id =
            database.query_row("SELECT user_id FROM identity WHERE id=1", [], |r| r.get(0))?;
        // A process loss cannot prove that a dispatched external action failed.
        database.execute(
            "UPDATE executions SET status='uncertain' WHERE status='running'",
            [],
        )?;
        Ok(Self {
            database: Mutex::new(database),
            gate: tokio::sync::RwLock::new(()),
            client,
            user_id,
        })
    }
    fn database(&self) -> Result<std::sync::MutexGuard<'_, Database>> {
        self.database
            .lock()
            .map_err(|_| anyhow::anyhow!("Business services unavailable"))
    }
    pub fn client(&self) -> Result<&composio::Composio> {
        self.client
            .as_ref()
            .context("Configure COMPOSIO_API_KEY locally and restart Ayati")
    }
    pub fn executions(&self, agent: &str) -> Result<serde_json::Value> {
        let db = self.database()?;
        let mut query=db.prepare("SELECT id,task_id,connection_id,tool,status,log_id,created_at FROM executions WHERE agent_id=?1 ORDER BY rowid DESC LIMIT 50")?;
        let rows=query.query_map([agent], |r|Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"task_id":r.get::<_,String>(1)?,"connection_id":r.get::<_,String>(2)?,"tool":r.get::<_,String>(3)?,"status":r.get::<_,String>(4)?,"log_id":r.get::<_,Option<String>>(5)?,"created_at":r.get::<_,String>(6)?})))?;
        Ok(serde_json::json!(
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        ))
    }
}
pub fn identifier(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 128
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b)),
        "Invalid service identifier"
    );
    Ok(())
}

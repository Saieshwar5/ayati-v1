use anyhow::{Result, ensure};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::{path::Path, sync::Mutex};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentProfile {
    pub id: String,
    pub name: String,
    pub role: String,
    pub model: String,
    pub memory: bool,
    pub browser: bool,
    pub state: String,
    pub detail: String,
    pub attempt: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewAgent {
    pub request_id: String,
    pub name: String,
    pub role: String,
    pub model: String,
    pub memory: bool,
    pub browser: bool,
}

impl NewAgent {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            uuid::Uuid::parse_str(&self.request_id)
                .is_ok_and(|id| id.to_string() == self.request_id),
            "Request ID must be a canonical UUID"
        );
        for (label, text, max) in [
            ("Name", &self.name, 80),
            ("Role", &self.role, 4000),
            ("Model", &self.model, 200),
        ] {
            ensure!(
                !text.trim().is_empty() && text.len() <= max,
                "{label} is empty or too long"
            );
            ensure!(
                !text.chars().any(|c| c.is_control() && c != '\n'),
                "{label} contains control characters"
            );
        }
        ensure!(
            !self.model.chars().any(char::is_whitespace),
            "Model cannot contain whitespace"
        );
        Ok(())
    }
}

pub struct Registry(Mutex<Connection>);
impl Registry {
    pub fn open(path: &Path) -> Result<Self> {
        let connection = Connection::open(path)?;
        connection.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
            CREATE TABLE IF NOT EXISTS agents(id TEXT PRIMARY KEY, profile TEXT NOT NULL);",
        )?;
        let registry = Self(Mutex::new(connection));
        for mut profile in registry.list()? {
            if profile.state == "preparing" {
                profile.state = "failed".into();
                profile.detail = "Setup was interrupted. Retry builds a fresh environment.".into();
                registry.save(&profile)?;
            }
        }
        Ok(registry)
    }
    pub fn list(&self) -> Result<Vec<AgentProfile>> {
        let connection = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("Agent registry unavailable"))?;
        let mut query = connection.prepare("SELECT profile FROM agents ORDER BY rowid")?;
        let rows = query.query_map([], |r| r.get::<_, String>(0))?;
        rows.map(|row| Ok(serde_json::from_str(&row?)?)).collect()
    }
    pub fn save(&self, profile: &AgentProfile) -> Result<()> {
        self.0.lock().map_err(|_| anyhow::anyhow!("Agent registry unavailable"))?.execute(
            "INSERT INTO agents(id,profile) VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET profile=excluded.profile",
            params![profile.id, serde_json::to_string(profile)?],
        )?;
        Ok(())
    }
}

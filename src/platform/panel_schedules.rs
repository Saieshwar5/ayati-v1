use super::Platform;
use anyhow::{Result, ensure};
use rusqlite::params;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PanelSchedule {
    pub id: String,
    pub content: String,
    pub run_at_ms: i64,
    pub interval_seconds: Option<i64>,
    pub panel_id: Option<String>,
}
impl Platform {
    pub fn ensure_panel_owner(&self, agent: &str, id: &str) -> Result<()> {
        let exists: bool = self.database()?.query_row(
            "SELECT EXISTS(SELECT 1 FROM panels WHERE id=?1 AND agent_id=?2 AND state='saved')",
            params![id, agent],
            |r| r.get(0),
        )?;
        ensure!(exists, "Panel is unavailable to this agent");
        Ok(())
    }
    pub fn bind_panel_schedule(&self, agent: &str, id: &str, schedule: &str) -> Result<()> {
        self.ensure_panel_owner(agent, id)?;
        self.database()?.execute(
            "INSERT OR IGNORE INTO panel_schedules VALUES(?1,?2,?3)",
            params![id, agent, schedule],
        )?;
        let existing: String = self.database()?.query_row(
            "SELECT panel_id FROM panel_schedules WHERE agent_id=?1 AND schedule_id=?2",
            params![agent, schedule],
            |r| r.get(0),
        )?;
        ensure!(
            existing == id,
            "Schedule is already bound to a different panel"
        );
        Ok(())
    }
    pub fn panel_schedules(&self, id: &str) -> Result<Vec<(String, String)>> {
        let db = self.database()?;
        let mut q =
            db.prepare("SELECT agent_id,schedule_id FROM panel_schedules WHERE panel_id=?1")?;
        Ok(q.query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?)
    }
    pub fn fence_panel_schedules(&self, agent: &str, store: &crate::storage::Store) -> Result<()> {
        // Read only policy/source metadata; do not deserialize every panel snapshot
        // on the one-second scheduler tick.
        let bindings: Vec<(String, bool, Option<String>)> = {
            let db = self.database()?;
            let mut q=db.prepare("SELECT s.schedule_id,(p.updates_enabled=1 AND p.state='saved' AND p.active_revision=p.revision),v.sources FROM panel_schedules s JOIN panels p ON p.id=s.panel_id LEFT JOIN panel_versions v ON v.panel_id=p.id AND v.revision=p.active_revision WHERE s.agent_id=?1")?;
            q.query_map([agent], |r| {
                Ok((
                    r.get(0)?,
                    r.get::<_, Option<bool>>(1)?.unwrap_or(false),
                    r.get(2)?,
                ))
            })?
            .collect::<rusqlite::Result<_>>()?
        };
        let schedules = store.schedules()?;
        for (id, enabled, sources) in bindings {
            let allowed = enabled
                && sources
                    .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
                    .is_some_and(|s| self.panel_source_grants(agent, &s).is_ok());
            if !allowed && schedules.iter().any(|s| s.id == id && s.state == "active") {
                store.set_schedule_enabled(&id, false)?;
            }
        }
        Ok(())
    }
}

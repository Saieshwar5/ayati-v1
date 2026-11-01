use crate::panels::Section;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Agreement {
    pub request: String,
    pub revision: i64,
    pub panel_revision: i64,
    pub enabled: bool,
    pub instructions: String,
    pub interval_seconds: Option<i64>,
    pub stale_seconds: i64,
    /// Also update when the agent's work or an event touches the panel's sources.
    #[serde(default)]
    pub follow_sources: Option<bool>,
    /// At most one update every this many seconds (1 minute to 24 hours).
    #[serde(default)]
    pub min_gap_seconds: Option<i64>,
}
impl Agreement {
    pub(crate) fn validate(&self) -> Result<()> {
        super::super::automations::model::uuid(&self.request)?;
        ensure!(
            self.revision >= 0 && self.panel_revision > 0,
            "Invalid agreement revision"
        );
        ensure!(
            !self.instructions.trim().is_empty()
                && self.instructions.len() <= 2000
                && !self.instructions.contains('\0'),
            "Describe the update in at most 2,000 bytes"
        );
        ensure!(
            (60..=604800).contains(&self.stale_seconds)
                && self
                    .interval_seconds
                    .is_none_or(|v| (60..=86400).contains(&v) && v <= self.stale_seconds),
            "Choose a 1 minute–24 hour interval and a maximum age at least as long (up to 7 days)"
        );
        ensure!(
            self.min_gap_seconds
                .is_none_or(|v| (60..=86400).contains(&v)),
            "Updates can be at most once a minute and at least once a day apart"
        );
        Ok(())
    }
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Update {
    pub request: String,
    pub revision: i64,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Refresh {
    pub sections: Vec<Section>,
    /// The agent's short note on what changed since the last update, shown on
    /// the panel. Only when something meaningful changed; at most 280 characters.
    #[serde(default)]
    pub what_changed: Option<String>,
    /// How each value comes from the reviewed reads, so the controller can refresh
    /// the numbers itself. Kept only when they reproduce this update exactly.
    #[serde(default)]
    pub bindings: Option<Vec<crate::panels::bindings::Binding>>,
}

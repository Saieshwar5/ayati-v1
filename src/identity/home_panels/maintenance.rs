use crate::platform::panels::Section;
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
}

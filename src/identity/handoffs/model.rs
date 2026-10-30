use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    pub key: String,
    pub agent: String,
    pub task: String,
}
impl Request {
    pub fn validate(&self) -> Result<()> {
        crate::identity::identifier(&self.key)?;
        ensure!(
            self.key.len() <= 64,
            "Use a stable handoff key up to 64 bytes"
        );
        crate::identity::automations::model::uuid(&self.agent)?;
        ensure!(
            !self.task.trim().is_empty() && self.task.len() <= 4000 && !self.task.contains('\0'),
            "Use a handoff task up to 4,000 bytes"
        );
        Ok(())
    }
}

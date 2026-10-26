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
        crate::platform::identifier(&self.key)?;
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
pub(crate) fn wait_seconds() -> Result<i64> {
    let n = std::env::var("AYATI_HANDOFF_WAIT_SECONDS")
        .unwrap_or_else(|_| "604800".into())
        .parse::<i64>()?;
    ensure!((60..=2592000).contains(&n), "Invalid handoff wait bound");
    Ok(n)
}

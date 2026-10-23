//! Bounded local executor protocol. A profile is compatibility, never authority.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Profile {
    pub provider: String,
    pub model: String,
}
impl Profile {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            ["openai", "fireworks-ai"].contains(&self.provider.as_str())
                && !self.model.is_empty()
                && self.model.len() <= 200,
            "Invalid executor profile"
        );
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SlotReport {
    pub slot: String,
    pub generation: i64,
    pub profile: Profile,
    pub state: String,
    pub token: String,
    pub pid: Option<u32>,
    #[serde(default)]
    pub preparation_version: u8,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Capacity {
    pub slots: Vec<SlotReport>,
}

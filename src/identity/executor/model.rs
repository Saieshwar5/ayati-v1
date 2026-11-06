//! Bounded local executor protocol. A profile is compatibility, never authority.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Profile {
    pub provider: String,
    pub model: String,
}
/// The provider a gateway executor serving any catalog model reports.
pub(crate) const ANY_PROVIDER: &str = "gateway";
impl Profile {
    /// The profile of a gateway executor that serves any catalog model (MG1): the
    /// controller chooses the model and provider for every call.
    pub fn any() -> Self {
        Self {
            provider: ANY_PROVIDER.into(),
            model: "*".into(),
        }
    }
    pub fn serves_any(&self) -> bool {
        self.provider == ANY_PROVIDER
    }
    pub fn validate(&self) -> Result<()> {
        if self.serves_any() {
            ensure!(self.model == "*", "Invalid executor profile");
            return Ok(());
        }
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

use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Source {
    pub id: String,
    pub revision: i64,
    pub access_revision: i64,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Start {
    pub id: String,
    pub agent_revision: i64,
    pub task: String,
    #[serde(default)]
    pub sources: Vec<Source>,
}
impl Start {
    pub(crate) fn validate(&self) -> Result<()> {
        uuid(&self.id)?;
        ensure!(
            self.agent_revision > 0
                && !self.task.trim().is_empty()
                && self.task.len() <= 4000
                && !self.task.contains('\0'),
            "Use a saved agent and a sample task up to 4,000 bytes"
        );
        ensure!(
            self.sources.len() <= 3,
            "Select up to three knowledge sources"
        );
        let mut ids = std::collections::HashSet::new();
        for source in &self.sources {
            uuid(&source.id)?;
            ensure!(
                source.revision > 0 && source.access_revision > 0 && ids.insert(&source.id),
                "Invalid or duplicate knowledge source"
            );
        }
        Ok(())
    }
}
pub(crate) fn uuid(id: &str) -> Result<()> {
    ensure!(uuid::Uuid::parse_str(id).is_ok(), "Invalid sample identity");
    Ok(())
}
pub(crate) fn fingerprint(input: &Start) -> Result<String> {
    let mut value = serde_json::to_value(input)?;
    value.sort_all_objects();
    Ok(crate::identity::provider::digest(&serde_json::to_string(
        &value,
    )?))
}

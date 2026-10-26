//! Declarative personal preferences and explicit shared channel lifecycle input.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PinKind {
    Chat,
    LocalAgent,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(deny_unknown_fields)]
pub(crate) struct Pin {
    pub kind: PinKind,
    pub id: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SavePins {
    pub request: String,
    pub revision: i64,
    pub pins: Vec<Pin>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Lifecycle {
    pub request: String,
    pub revision: i64,
    pub archived: bool,
}
impl SavePins {
    pub(crate) fn validate(&self) -> Result<()> {
        validate(&self.request, self.revision)?;
        ensure!(self.pins.len() <= 30, "Pin at most 30 conversations");
        let mut seen = std::collections::HashSet::new();
        for pin in &self.pins {
            ensure!(
                !pin.id.is_empty() && pin.id.len() <= 128 && !pin.id.chars().any(char::is_control),
                "Invalid pin identity"
            );
            ensure!(seen.insert(pin), "A conversation can only be pinned once");
        }
        Ok(())
    }
}
impl Lifecycle {
    pub(crate) fn validate(&self) -> Result<()> {
        validate(&self.request, self.revision)
    }
}
fn validate(request: &str, revision: i64) -> Result<()> {
    ensure!(
        uuid::Uuid::parse_str(request).is_ok() && revision >= 0,
        "Invalid request identity or revision"
    );
    Ok(())
}

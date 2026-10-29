//! Home panel definitions: the one data contract agents and people review.
//! Storage, review and refresh live in the identity PostgreSQL store.
pub mod presentation;
mod sections;
mod templates;
#[cfg(test)]
mod templates_tests;
mod validation;

use anyhow::{Result, ensure};
pub use sections::Section;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PanelDraft {
    pub id: String,
    pub expected_revision: i64,
    pub title: String,
    pub purpose: String,
    pub sections: Vec<Section>,
    #[serde(default)]
    pub template: Option<templates::Template>,
    pub sources: Vec<String>,
    #[serde(default)]
    pub publish: bool,
    #[serde(default)]
    pub updates_enabled: bool,
}
impl PanelDraft {
    pub(crate) fn validate(&self) -> Result<()> {
        self.template()?;
        ensure!(
            !self.updates_enabled || self.publish,
            "An update policy must be approved with publication"
        );
        ensure!(
            uuid::Uuid::parse_str(&self.id).is_ok_and(|id| id.to_string() == self.id),
            "Panel ID must be a canonical UUID"
        );
        ensure!(
            self.expected_revision >= 0
                && !self.title.trim().is_empty()
                && self.title.len() <= 100
                && self.purpose.len() <= 1000,
            "Panel title/purpose or revision is invalid"
        );
        ensure!(
            !self.sections.is_empty()
                && self.sections.len() <= 16
                && self.sources.len() <= 16
                && serde_json::to_vec(self)?.len() <= 12000,
            "Panel exceeds the saved view budget"
        );
        Ok(())
    }
    pub(crate) fn definition(&self) -> Value {
        json!({"title":self.title,"purpose":self.purpose,"sections":self.sections,"template":self.template().ok()})
    }
}

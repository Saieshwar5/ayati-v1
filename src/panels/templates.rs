//! General Home families with aliases for existing saved definitions.
use super::{PanelDraft, Section};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Template {
    Metric,
    Table,
    List,
    Chart,
    Agenda,
    Brief,
    Text,
    Collection,
    Progress,
    Timeline,
}
impl PanelDraft {
    pub fn template(&self) -> Result<Template> {
        ensure!(
            self.sections.len() == 1,
            "Choose one general Home family with exactly one section: metric, chart, table, collection, progress, timeline or text"
        );
        let inferred = match &self.sections[0] {
            Section::Metric { .. } | Section::Metrics { .. } => Template::Metric,
            Section::Table { .. } => Template::Table,
            Section::Chart { .. } => Template::Chart,
            Section::Text { .. } => Template::Text,
            Section::Collection { .. } => Template::Collection,
            Section::Progress { .. } => Template::Progress,
            Section::Timeline { .. } => Template::Timeline,
        };
        let template = self.template.unwrap_or(inferred);
        ensure!(
            template == inferred
                || (matches!(template, Template::List | Template::Agenda)
                    && inferred == Template::Table)
                || (template == Template::Brief && inferred == Template::Text),
            "Section does not match the selected panel template"
        );
        super::validation::validate(&self.sections[0])?;
        Ok(template)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn templates_bound_agent_content_and_infer_old_single_sections() {
        let mut value = json!({"id":"00000000-0000-0000-0000-000000000001","expected_revision":0,"title":"Stock","purpose":"","sources":[],"sections":[{"kind":"metric","label":"Items","value":"4"}]});
        let parse = |v| serde_json::from_value::<PanelDraft>(v).unwrap().template();
        assert!(parse(value.clone()).unwrap() == Template::Metric);
        value["template"] = json!("agenda");
        assert!(parse(value.clone()).is_err());
        value["template"] = json!("metric");
        value["sections"][0]["value"] = json!("x".repeat(41));
        assert!(parse(value.clone()).is_err());
        value["sections"] = json!([]);
        assert!(parse(value).is_err());
    }
}

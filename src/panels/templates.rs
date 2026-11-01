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
    Pipeline,
}
impl PanelDraft {
    pub fn template(&self) -> Result<Template> {
        // Up to three sections (HP6); the first one names the panel's family.
        ensure!(
            (1..=3).contains(&self.sections.len()),
            "Choose one general Home family with one to three sections: metric, chart, table, collection, progress, timeline, pipeline or text"
        );
        let inferred = match &self.sections[0] {
            Section::Metric { .. } | Section::Metrics { .. } => Template::Metric,
            Section::Table { .. } => Template::Table,
            Section::Chart { .. } => Template::Chart,
            Section::Text { .. } => Template::Text,
            Section::Collection { .. } => Template::Collection,
            Section::Progress { .. } => Template::Progress,
            Section::Timeline { .. } => Template::Timeline,
            Section::Pipeline { .. } => Template::Pipeline,
        };
        let template = self.template.unwrap_or(inferred);
        ensure!(
            template == inferred
                || (matches!(template, Template::List | Template::Agenda)
                    && inferred == Template::Table)
                || (template == Template::Brief && inferred == Template::Text),
            "Section does not match the selected panel template"
        );
        for section in &self.sections {
            super::validation::validate(section)?;
        }
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
        assert!(parse(value.clone()).is_err());
        // Up to three sections; the first names the family. A pipeline is its own.
        value["template"] = json!(null);
        let pipeline = json!({"kind":"pipeline","stages":[{"id":"lead","label":"Leads","count":12},{"id":"quote","label":"Quoted","count":5,"value":"₹4.2L"},{"id":"won","label":"Won","count":2}]});
        let metric = json!({"kind":"metric","label":"Revenue","value":"₹48,200"});
        value["sections"] = json!([pipeline, metric]);
        assert!(parse(value.clone()).unwrap() == Template::Pipeline);
        value["sections"] = json!([metric, pipeline, metric, metric]);
        assert!(parse(value.clone()).is_err(), "at most three");
        value["sections"] =
            json!([{"kind":"pipeline","stages":[{"id":"a","label":"A","count":-1}]}]);
        assert!(parse(value).is_err());
    }
}

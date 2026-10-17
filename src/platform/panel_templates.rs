//! Fixed Home templates. Agents supply data; owners control placement and size.
use super::panels::{PanelDraft, Section};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Template {
    Metric,
    List,
    Chart,
    Agenda,
    Brief,
}
impl PanelDraft {
    pub fn template(&self) -> Result<Template> {
        ensure!(
            self.sections.len() == 1,
            "Choose one Home template with exactly one section: metric, list (table), chart, agenda (table), or brief (text)"
        );
        let inferred = match &self.sections[0] {
            Section::Metric { .. } => Template::Metric,
            Section::Table { .. } => Template::List,
            Section::Chart { .. } => Template::Chart,
            Section::Text { .. } => Template::Brief,
        };
        let template = self.template.unwrap_or(inferred);
        ensure!(
            template == inferred || (template == Template::Agenda && inferred == Template::List),
            "Section does not match the selected panel template"
        );
        match &self.sections[0] {
            Section::Metric { label, value } => ensure!(
                label.len() <= 80 && value.len() <= 40,
                "Metric supports an 80-byte label and 40-byte value"
            ),
            Section::Table { columns, rows } => ensure!(
                columns.len() <= 3
                    && rows.len() <= 8
                    && columns.iter().all(|v| v.len() <= 60)
                    && rows.iter().flatten().all(|v| v.len() <= 160),
                "List/agenda supports up to 3 columns, 8 rows and 160 bytes per cell"
            ),
            Section::Chart { title, points } => ensure!(
                title.len() <= 100
                    && points.len() <= 12
                    && points.iter().all(|p| p.label.len() <= 40),
                "Chart supports up to 12 points and 40-byte labels"
            ),
            Section::Text { heading, body } => ensure!(
                heading.len() <= 100 && body.len() <= 800,
                "Brief supports a 100-byte heading and 800-byte body"
            ),
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
        assert!(parse(value).is_err());
    }
}

//! One advertised data contract for managed personal panel tools.
use serde_json::Value;
pub const TEMPLATES: &[&str] = &[
    "metric",
    "chart",
    "table",
    "collection",
    "progress",
    "timeline",
    "text",
    "pipeline",
    "list",
    "agenda",
    "brief",
];
pub const GUIDE: &str = concat!(
    "Home supports general reusable families: Metric, Chart, Table, Collection, Progress, Timeline, Pipeline and Text. ",
    "Business purposes are configurations, never new UI types. Supply one to three sections (the first names the family) with kind: ",
    "pipeline {stages:[{id,label,count,value?}]} for 1–8 ordered stages (count >= 0, value<=20 bytes, e.g. ₹4.2L); ",
    "metric {label,value,comparison?}; metrics {items:[{id,label,value,comparison?}]} for 1–4 measures; ",
    "table {columns:[string],rows:[[string]],column_types?:[text|number]} with 1–3 columns/0–8 matching rows; ",
    "chart {title,points:[{label,value}],variant?,unit?} OR chart {title,series:[{name,points:[{label,value}]}],variant?,unit?}. ",
    "Chart variants: line (default), bar, area, stacked_bar, donut. At most 4 uniquely named series/12 points each; ",
    "series have identical ordered category labels. Donut has one non-negative series; stacked_bar requires non-negative values. ",
    "collection {items:[{id,title,description?,status?}]} supports 0–8 records; ",
    "progress {items:[{id,label,actual,target,minimum?,unit?}]} supports 1–4 measures, target > minimum (default 0); ",
    "timeline {events:[{id,at_ms,label,description?}]} supports 0–8 events with Unix millisecond dates; ",
    "text {heading,body} supports heading<=100/body<=800 bytes. Items require unique stable IDs. ",
    "Metric labels<=80/value strings<=40 bytes. Comparison is {label,value:number,unit?,favorable?:increase|decrease|neutral}; ",
    "favorable describes the good direction, not the sign of the current change. Table cells<=160 bytes; numeric cells are unformatted finite numbers as strings. ",
    "Chart labels<=40 bytes; unit<=40 bytes. All numeric data must be finite. Empty records/charts are explicit empty states, never fabricated zeros. ",
    "Optional template names the matching family; list/agenda alias table and brief aliases text for compatibility. ",
    "No arbitrary UI, item URLs, HTML, scripts or business actions. Human controls placement. Whole payload<=12000 bytes. ",
    "Refreshes preserve the reviewed family, labels, units, variants, series names, columns, comparison direction and targets. ",
    "Only values and records change without a new definition review."
);

/// Exact argument shape for saving a Home panel draft.
pub fn panel_write_input(input: &Value) -> bool {
    let Some(input) = input.as_object() else {
        return false;
    };
    (6..=9).contains(&input.len())
        && input.keys().all(|key| {
            [
                "id",
                "title",
                "purpose",
                "expected_revision",
                "sections",
                "template",
                "sources",
                "publish",
                "updates_enabled",
            ]
            .contains(&key.as_str())
        })
        && input
            .get("template")
            .is_none_or(|v| v.as_str().is_some_and(|s| TEMPLATES.contains(&s)))
        && ["publish", "updates_enabled"]
            .iter()
            .all(|key| input.get(*key).is_none_or(Value::is_boolean))
        && ["id", "title", "purpose"]
            .iter()
            .all(|k| input.get(*k).is_some_and(Value::is_string))
        && input
            .get("expected_revision")
            .is_some_and(|v| v.as_i64().is_some())
        && ["sections", "sources"]
            .iter()
            .all(|k| input.get(*k).is_some_and(Value::is_array))
}

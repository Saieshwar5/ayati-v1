//! Data-only structures for reusable Home renderers; no agent UI or action code.
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Section {
    Text {
        heading: String,
        body: String,
    },
    Metric {
        label: String,
        value: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        comparison: Option<Comparison>,
    },
    Metrics {
        items: Vec<Metric>,
    },
    Table {
        columns: Vec<String>,
        rows: Vec<Vec<String>>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        column_types: Vec<ColumnType>,
    },
    Chart {
        title: String,
        #[serde(default)]
        points: Vec<Point>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        variant: Option<ChartVariant>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        series: Vec<Series>,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        unit: String,
    },
    Collection {
        items: Vec<Item>,
    },
    Progress {
        items: Vec<Progress>,
    },
    Timeline {
        events: Vec<Event>,
    },
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Point {
    pub label: String,
    pub value: f64,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Series {
    pub name: String,
    pub points: Vec<Point>,
}
#[derive(Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChartVariant {
    Line,
    Bar,
    Area,
    StackedBar,
    Donut,
}
#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ColumnType {
    Text,
    Number,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Comparison {
    pub label: String,
    pub value: f64,
    #[serde(default)]
    pub unit: String,
    #[serde(default)]
    pub favorable: FavorableDirection,
}
#[derive(Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FavorableDirection {
    Increase,
    Decrease,
    #[default]
    Neutral,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Metric {
    pub id: String,
    pub label: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comparison: Option<Comparison>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Item {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub status: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Progress {
    pub id: String,
    pub label: String,
    pub actual: f64,
    pub target: f64,
    #[serde(default)]
    pub minimum: f64,
    #[serde(default)]
    pub unit: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub id: String,
    pub at_ms: i64,
    pub label: String,
    #[serde(default)]
    pub description: String,
}

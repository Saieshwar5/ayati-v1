use super::panel_sections::*;
use anyhow::{Result, ensure};
use std::collections::HashSet;

fn text(value: &str, max: usize) -> bool {
    value.len() <= max && !value.contains('\0')
}
fn ids<'a>(values: impl Iterator<Item = &'a str>) -> bool {
    let mut seen = HashSet::new();
    values
        .into_iter()
        .all(|id| !id.trim().is_empty() && text(id, 80) && seen.insert(id))
}
fn comparison(value: &Option<Comparison>) -> bool {
    value
        .as_ref()
        .is_none_or(|v| text(&v.label, 100) && v.value.is_finite() && text(&v.unit, 40))
}
fn points(values: &[Point]) -> bool {
    values.len() <= 12
        && values
            .iter()
            .all(|p| p.value.is_finite() && text(&p.label, 40))
}

pub(super) fn validate(section: &Section) -> Result<()> {
    match section {
        Section::Text { heading, body } => ensure!(
            text(heading, 100) && text(body, 800),
            "Text supports a 100-byte heading and 800-byte body"
        ),
        Section::Metric {
            label,
            value,
            comparison: c,
        } => ensure!(
            text(label, 80) && text(value, 40) && comparison(c),
            "Metric supports an 80-byte label, 40-byte value and bounded comparison"
        ),
        Section::Metrics { items } => ensure!(
            (1..=4).contains(&items.len())
                && ids(items.iter().map(|v| v.id.as_str()))
                && items
                    .iter()
                    .all(|v| text(&v.label, 80) && text(&v.value, 40) && comparison(&v.comparison)),
            "Metric groups support 1–4 uniquely identified measures with bounded labels/values"
        ),
        Section::Table {
            columns,
            rows,
            column_types,
        } => {
            ensure!(
                (1..=3).contains(&columns.len())
                    && rows.len() <= 8
                    && columns.iter().all(|v| text(v, 60))
                    && rows
                        .iter()
                        .all(|r| r.len() == columns.len() && r.iter().all(|v| text(v, 160))),
                "Table supports 1–3 columns, 8 matching rows and 160 bytes per cell"
            );
            ensure!(
                column_types.is_empty() || column_types.len() == columns.len(),
                "Column types must match the table columns"
            );
            for (index, kind) in column_types.iter().enumerate() {
                if matches!(kind, ColumnType::Number) {
                    ensure!(
                        rows.iter().all(|r| r[index].trim().is_empty()
                            || r[index].parse::<f64>().is_ok_and(f64::is_finite)),
                        "Numeric cells must be empty or finite unformatted numbers"
                    );
                }
            }
        }
        Section::Chart {
            title,
            points: data,
            variant,
            series,
            unit,
        } => {
            ensure!(
                text(title, 100) && text(unit, 40) && points(data) && series.len() <= 4,
                "Chart supports a 100-byte title, 40-byte unit and up to 4 series of 12 points"
            );
            ensure!(
                data.is_empty() || series.is_empty(),
                "Choose points or series, not both"
            );
            let mut names = HashSet::new();
            for item in series {
                ensure!(
                    !item.name.trim().is_empty()
                        && text(&item.name, 60)
                        && names.insert(&item.name)
                        && points(&item.points),
                    "Invalid or duplicate chart series"
                );
                ensure!(
                    item.points
                        .iter()
                        .map(|p| &p.label)
                        .eq(series[0].points.iter().map(|p| &p.label)),
                    "Series must use the same ordered category labels"
                );
            }
            let variant = variant.unwrap_or(ChartVariant::Line);
            if matches!(variant, ChartVariant::Donut | ChartVariant::StackedBar) {
                ensure!(
                    data.iter()
                        .chain(series.iter().flat_map(|s| &s.points))
                        .all(|p| p.value >= 0.),
                    "Donut and stacked bars require non-negative values"
                );
            }
            if variant == ChartVariant::Donut {
                ensure!(series.len() <= 1, "Donut accepts one series");
                let total: f64 = data
                    .iter()
                    .chain(series.iter().flat_map(|s| &s.points))
                    .map(|p| p.value)
                    .sum();
                ensure!(total.is_finite(), "Donut total must be finite");
            }
            if variant == ChartVariant::StackedBar {
                for index in 0..series.first().map_or(0, |s| s.points.len()) {
                    ensure!(
                        series
                            .iter()
                            .map(|s| s.points[index].value)
                            .sum::<f64>()
                            .is_finite(),
                        "Stacked totals must be finite"
                    );
                }
            }
        }
        Section::Collection { items } => ensure!(
            items.len() <= 8
                && ids(items.iter().map(|v| v.id.as_str()))
                && items.iter().all(|v| text(&v.title, 100)
                    && text(&v.description, 240)
                    && text(&v.status, 40)),
            "Collection supports up to 8 uniquely identified items with bounded text"
        ),
        Section::Progress { items } => ensure!(
            (1..=4).contains(&items.len())
                && ids(items.iter().map(|v| v.id.as_str()))
                && items.iter().all(|v| text(&v.label, 80)
                    && text(&v.unit, 40)
                    && v.actual.is_finite()
                    && v.target.is_finite()
                    && v.minimum.is_finite()
                    && v.target > v.minimum),
            "Progress supports 1–4 unique measures with finite actual/range and target greater than minimum"
        ),
        Section::Timeline { events } => ensure!(
            events.len() <= 8
                && ids(events.iter().map(|v| v.id.as_str()))
                && events
                    .iter()
                    .all(
                        |v| (-8_640_000_000_000_000..=8_640_000_000_000_000).contains(&v.at_ms)
                            && text(&v.label, 100)
                            && text(&v.description, 240)
                    ),
            "Timeline supports up to 8 unique events with valid millisecond dates and bounded text"
        ),
    }
    Ok(())
}

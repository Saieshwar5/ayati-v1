use super::{PanelDraft, presentation::shape};
use serde_json::{Value, json};

fn draft(section: Value) -> PanelDraft {
    serde_json::from_value(json!({"id":"00000000-0000-4000-8000-000000000001",
        "expected_revision":0,"title":"General view","purpose":"","sections":[section],"sources":[]})).unwrap()
}
fn valid(section: Value) -> bool {
    let value = json!({"id":"00000000-0000-4000-8000-000000000001",
        "expected_revision":0,"title":"General view","purpose":"","sections":[section],"sources":[]});
    serde_json::from_value::<PanelDraft>(value).is_ok_and(|v| v.validate().is_ok())
}

#[test]
fn general_families_and_legacy_aliases_round_trip_without_rewriting_old_fields() {
    let examples = [
        (
            "metric",
            json!({"kind":"metric","label":"Count","value":"12"}),
        ),
        (
            "metric",
            json!({"kind":"metrics","items":[{"id":"a","label":"A","value":"12"}]}),
        ),
        (
            "table",
            json!({"kind":"table","columns":["Name"],"rows":[["A"]]}),
        ),
        (
            "chart",
            json!({"kind":"chart","title":"Trend","points":[{"label":"A","value":1}]}),
        ),
        ("collection", json!({"kind":"collection","items":[]})),
        (
            "progress",
            json!({"kind":"progress","items":[{"id":"a","label":"A","actual":72,"target":100}]}),
        ),
        ("timeline", json!({"kind":"timeline","events":[]})),
        (
            "text",
            json!({"kind":"text","heading":"Context","body":"Facts"}),
        ),
    ];
    for (family, section) in examples {
        let value = draft(section);
        value.validate().unwrap();
        assert_eq!(value.definition()["template"], family);
    }
    for alias in ["list", "agenda", "brief"] {
        let section = if alias == "brief" {
            json!({"kind":"text","heading":"A","body":"B"})
        } else {
            json!({"kind":"table","columns":["A"],"rows":[]})
        };
        let mut value = serde_json::to_value(draft(section.clone())).unwrap();
        value["template"] = json!(alias);
        let value: PanelDraft = serde_json::from_value(value).unwrap();
        value.validate().unwrap();
        assert_eq!(value.definition()["sections"][0], section);
        assert_eq!(value.definition()["template"], alias);
    }
}

#[test]
fn charts_validate_series_alignment_variants_totals_and_empty_states() {
    for variant in ["line", "bar", "area", "stacked_bar", "donut"] {
        assert!(valid(
            json!({"kind":"chart","title":"A","variant":variant,"points":[{"label":"A","value":2}]})
        ));
    }
    assert!(valid(json!({"kind":"chart","title":"Empty","points":[]})));
    let a = json!({"name":"A","points":[{"label":"One","value":1}]});
    let b = json!({"name":"B","points":[{"label":"One","value":2}]});
    assert!(valid(json!({"kind":"chart","title":"A","series":[a,b]})));
    for bad in [
        json!({"kind":"chart","title":"A","points":[{"label":"A","value":1}],"series":[a]}),
        json!({"kind":"chart","title":"A","series":[a,a]}),
        json!({"kind":"chart","title":"A","series":[a,{"name":"B","points":[{"label":"Different","value":1}]}]}),
        json!({"kind":"chart","title":"A","variant":"donut","series":[a,b]}),
        json!({"kind":"chart","title":"A","variant":"donut","points":[{"label":"A","value":-1}]}),
        json!({"kind":"chart","title":"A","variant":"stacked_bar","points":[{"label":"A","value":-1}]}),
        json!({"kind":"chart","title":"A","variant":"donut","points":[{"label":"A","value":f64::MAX},{"label":"B","value":f64::MAX}]}),
        json!({"kind":"chart","title":"A","variant":"pie","points":[]}),
    ] {
        assert!(!valid(bad));
    }
}

#[test]
fn records_ranges_timestamps_and_numeric_cells_reject_ambiguous_or_unbounded_data() {
    assert!(valid(
        json!({"kind":"table","columns":["N"],"column_types":["number"],"rows":[["10"],[""]]})
    ));
    for section in [
        json!({"kind":"table","columns":["N"],"column_types":["number"],"rows":[["NaN"]]}),
        json!({"kind":"table","columns":["N"],"column_types":["text","number"],"rows":[]}),
        json!({"kind":"table","columns":["A","B"],"rows":[["A"]]}),
        json!({"kind":"collection","items":[{"id":"a","title":"A"},{"id":"a","title":"B"}]}),
        json!({"kind":"collection","items":[{"id":"a","title":"A","url":"javascript:alert(1)"}]}),
        json!({"kind":"progress","items":[{"id":"a","label":"A","actual":10,"target":0}]}),
        json!({"kind":"timeline","events":[{"id":"a","at_ms":8_640_000_000_000_001_i64,"label":"A"}]}),
        json!({"kind":"text","heading":"A","body":"x".repeat(801)}),
        json!({"kind":"metric","label":"A","value":"1","comparison":{"label":"B","value":"8%"}}),
    ] {
        assert!(!valid(section));
    }
}

#[test]
fn refresh_shape_changes_data_but_keeps_targets_units_columns_and_interpretation() {
    let progress = draft(json!({"kind":"progress","items":[{"id":"a","label":"A","actual":72,"target":100,"unit":"count"}]})).definition();
    let mut next = progress.clone();
    next["sections"][0]["items"][0]["actual"] = json!(80);
    assert_eq!(shape(progress.clone()), shape(next.clone()));
    next["sections"][0]["items"][0]["target"] = json!(200);
    assert_ne!(shape(progress), shape(next));
    let chart = draft(json!({"kind":"chart","title":"Trend","variant":"line","unit":"%","series":[{"name":"A","points":[{"label":"Mon","value":1}]}]})).definition();
    let mut next = chart.clone();
    next["sections"][0]["series"][0]["points"] = json!([{"label":"Tue","value":2}]);
    assert_eq!(shape(chart.clone()), shape(next.clone()));
    for (field, value) in [("variant", json!("bar")), ("unit", json!("count"))] {
        let mut changed = next.clone();
        changed["sections"][0][field] = value;
        assert_ne!(shape(chart.clone()), shape(changed));
    }
    next["sections"][0]["series"][0]["name"] = json!("Changed meaning");
    assert_ne!(shape(chart), shape(next));
    let metric = draft(json!({"kind":"metric","label":"A","value":"1","comparison":{"label":"Period","value":8,"unit":"%","favorable":"increase"}})).definition();
    let mut next = metric.clone();
    next["sections"][0]["comparison"]["value"] = json!(-8);
    assert_eq!(shape(metric.clone()), shape(next.clone()));
    next["sections"][0]["comparison"]["favorable"] = json!("decrease");
    assert_ne!(shape(metric), shape(next));
}

#[test]
fn legacy_aliases_and_implicit_line_keep_the_same_reviewed_presentation() {
    for (old, canonical, section) in [
        (
            "list",
            "table",
            json!({"kind":"table","columns":["A"],"rows":[]}),
        ),
        (
            "agenda",
            "table",
            json!({"kind":"table","columns":["A"],"rows":[]}),
        ),
        (
            "brief",
            "text",
            json!({"kind":"text","heading":"A","body":"B"}),
        ),
    ] {
        let mut original = draft(section).definition();
        original["template"] = json!(old);
        let mut updated = original.clone();
        updated["template"] = json!(canonical);
        assert_eq!(shape(original), shape(updated));
    }
    let original = draft(json!({"kind":"chart","title":"A","points":[]})).definition();
    let mut updated = original.clone();
    updated["sections"][0]["variant"] = json!("line");
    assert_eq!(shape(original), shape(updated));
}

use super::*;
use serde_json::json;

fn bind(value: Value) -> Binding {
    serde_json::from_value(value).unwrap()
}

#[test]
fn values_counts_sums_and_rows_come_from_the_reads_without_a_model() {
    let reads = vec![
        json!({"content":[{"type":"text","text":"{\"orders\":[{\"id\":\"A-1\",\"amount\":1200.5,\"status\":\"paid\"},{\"id\":\"A-2\",\"amount\":\"47,000\",\"status\":\"due\"}],\"revenue\":48200.5}"}]}),
        json!({"structuredContent":{"tickets":{"open":14}}}),
    ];
    let metric = json!({"kind":"metric","label":"Revenue","value":"old"});
    let revenue = bind(
        json!({"target":"value","read":0,"path":"revenue","decimals":0,"group":"indian","prefix":"₹"}),
    );
    assert_eq!(
        apply(&metric, std::slice::from_ref(&revenue), &reads).unwrap()["value"],
        "₹48,200"
    );
    let metrics = json!({"kind":"metrics","items":[{"id":"o","label":"Orders","value":"0"},{"id":"t","label":"Taken","value":"0"},{"id":"k","label":"Open","value":"0"}]});
    let bindings = vec![
        bind(json!({"target":"items.0.value","read":0,"path":"orders","op":"count"})),
        bind(
            json!({"target":"items.1.value","read":0,"path":"orders","op":"sum","field":"amount","group":"thousands","decimals":2}),
        ),
        bind(json!({"target":"items.2.value","read":1,"path":"tickets.open"})),
    ];
    validate(&bindings, 2, &metrics).unwrap();
    let filled = apply(&metrics, &bindings, &reads).unwrap();
    assert_eq!(filled["items"][0]["value"], "2");
    assert_eq!(filled["items"][1]["value"], "48,200.50");
    assert_eq!(filled["items"][2]["value"], "14");
    let table = json!({"kind":"table","columns":["Order","Status"],"rows":[]});
    let rows = bind(json!({"target":"rows","read":0,"path":"orders","columns":["id","status"]}));
    assert_eq!(
        apply(&table, &[rows], &reads).unwrap()["rows"],
        json!([["A-1", "paid"], ["A-2", "due"]])
    );
    let progress =
        json!({"kind":"progress","items":[{"id":"p","label":"Target","actual":0.0,"target":20.0}]});
    let actual = bind(json!({"target":"items.0.actual","read":1,"path":"tickets.open"}));
    assert_eq!(
        apply(&progress, &[actual], &reads).unwrap()["items"][0]["actual"],
        14.0
    );
}

#[test]
fn bindings_are_kept_only_when_they_reproduce_what_was_published() {
    let reads = vec![json!({"count": 3})];
    let section = json!({"kind":"metric","label":"Orders","value":"3"});
    let b = vec![bind(json!({"target":"value","read":0,"path":"count"}))];
    check(&section, &b, &reads).unwrap();
    let wrong = json!({"kind":"metric","label":"Orders","value":"4"});
    assert!(
        check(&wrong, &b, &reads)
            .unwrap_err()
            .to_string()
            .contains("would be \"3\"")
    );
    assert!(
        check(
            &section,
            &[bind(json!({"target":"value","read":0,"path":"missing"}))],
            &reads
        )
        .is_err()
    );
}

#[test]
fn invalid_bindings_are_refused() {
    let metric = json!({"kind":"metric","label":"Orders","value":"3"});
    let ok = json!({"target":"value","read":0,"path":"count"});
    assert!(
        validate(&[bind(ok.clone())], 0, &metric).is_err(),
        "no such read"
    );
    assert!(
        validate(&[bind(ok.clone()), bind(ok.clone())], 1, &metric).is_err(),
        "bound twice"
    );
    assert!(
        validate(&[bind(json!({"target":"rows","read":0}))], 1, &metric).is_err(),
        "wrong kind"
    );
    assert!(
        validate(
            &[bind(json!({"target":"value","read":0,"op":"sum"}))],
            1,
            &metric
        )
        .is_err(),
        "sum without field"
    );
    assert!(
        validate(
            &[bind(json!({"target":"value","read":0,"decimals":3}))],
            1,
            &metric
        )
        .is_err()
    );
    assert!(
        serde_json::from_value::<Binding>(json!({"target":"value","read":0,"script":"x"})).is_err()
    );
}

#[test]
fn only_a_move_beyond_the_share_or_a_new_row_count_is_meaningful() {
    let b = vec![bind(
        json!({"target":"value","read":0,"path":"n","prefix":"₹"}),
    )];
    let at = |v: &str| json!({"kind":"metric","label":"Revenue","value":v});
    assert!(!meaningful(&at("₹48,200"), &at("₹49,000"), &b, 0.10));
    assert!(meaningful(&at("₹48,200"), &at("₹40,000"), &b, 0.10));
    let rows = vec![bind(json!({"target":"rows","read":0,"columns":["a"]}))];
    let table = |n: usize| json!({"kind":"table","columns":["A"],"rows":vec![vec!["x"]; n]});
    assert!(!meaningful(&table(2), &table(2), &rows, 0.10));
    assert!(meaningful(&table(2), &table(3), &rows, 0.10));
}

#[test]
fn numbers_format_like_people_write_them() {
    let b = |v: Value| bind(v);
    assert_eq!(
        format(
            4820000.0,
            &b(json!({"target":"value","read":0,"group":"indian"}))
        ),
        "48,20,000"
    );
    assert_eq!(
        format(
            4820000.0,
            &b(json!({"target":"value","read":0,"group":"thousands"}))
        ),
        "4,820,000"
    );
    assert_eq!(
        format(-12.5, &b(json!({"target":"value","read":0,"suffix":"%"}))),
        "-12.50%"
    );
    assert_eq!(format(7.0, &b(json!({"target":"value","read":0}))), "7");
}

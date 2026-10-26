use super::model::*;
use serde_json::{Value, json};
fn specification(value: Value) -> Specification {
    serde_json::from_value(value).unwrap()
}
fn choices(kind: &str) -> Specification {
    specification(
        json!({"version":1,"kind":kind,"title":"Delivery method","options":[{"id":"air","label":"Air"},{"id":"road","label":"Road"}]}),
    )
}
#[test]
fn closed_choices_reject_unknown_repeated_extra_or_missing_values() {
    let spec = choices("single_choice");
    spec.validate().unwrap();
    assert_eq!(
        spec.answer(&json!({"choices":["air"]})).unwrap(),
        json!({"choices":["air"]})
    );
    for value in [
        json!({"choices":[]}),
        json!({"choices":["air","road"]}),
        json!({"choices":["fake"]}),
        json!({"choices":["air","air"]}),
        json!({"choices":["air"],"other":"train"}),
        json!({"choices":["air"],"approve":true}),
    ] {
        assert!(spec.answer(&value).is_err(), "{value}");
    }
}
#[test]
fn multiple_choice_bounds_and_explicit_alternatives_preserve_stable_ids() {
    let mut spec = choices("multiple_choice");
    spec.min_choices = Some(2);
    spec.max_choices = Some(2);
    spec.allow_other = true;
    spec.validate().unwrap();
    assert_eq!(
        spec.answer(&json!({"choices":["road","air"]})).unwrap(),
        json!({"choices":["air","road"]})
    );
    assert!(spec.answer(&json!({"choices":["road"]})).is_err());
    assert!(
        spec.answer(&json!({"choices":["air","road"],"other":"Train"}))
            .is_err()
    );
    assert_eq!(
        spec.answer(&json!({"choices":["road"],"other":"Train"}))
            .unwrap(),
        json!({"choices":["road"],"other":"Train"})
    );
    let mut single = choices("single_choice");
    single.allow_other = true;
    assert!(
        single
            .answer(&json!({"choices":[],"other":"Train"}))
            .is_ok()
    );
    assert!(
        single
            .answer(&json!({"choices":["air"],"other":"Train"}))
            .is_err()
    );
}
fn form() -> Specification {
    specification(
        json!({"version":1,"kind":"form","title":"Quotation details","fields":[
    {"id":"name","label":"Customer name","kind":"text","required":true,"max_length":20},
    {"id":"notes","label":"Notes","kind":"multiline"},
    {"id":"quantity","label":"Quantity","kind":"number","min":0,"max":100,"required":true},
    {"id":"date","label":"Delivery date","kind":"date","required":true},
    {"id":"express","label":"Express delivery","kind":"boolean","required":true},
    {"id":"method","label":"Method","kind":"choice","options":[{"id":"road","label":"Road"}]}]}),
    )
}
#[test]
fn flat_forms_validate_each_type_false_zero_dates_and_optional_fields() {
    let spec = form();
    spec.validate().unwrap();
    let valid = json!({"name":"Customer","quantity":0,"date":"2028-02-29","express":false});
    assert_eq!(spec.answer(&valid).unwrap(), valid);
    for (id, value) in [
        ("name", json!(" ")),
        ("quantity", json!("2")),
        ("quantity", json!(101)),
        ("date", json!("2027-02-29")),
        ("date", json!("2028-04-31")),
        ("date", json!("+001-01-01")),
        ("express", json!("false")),
        ("method", json!("Unassigned")),
        ("extra", json!("injected")),
    ] {
        let mut invalid = valid.clone();
        invalid[id] = value;
        assert!(spec.answer(&invalid).is_err(), "{invalid}");
    }
    let mut missing = valid.clone();
    missing.as_object_mut().unwrap().remove("express");
    assert!(spec.answer(&missing).is_err());
}
#[test]
fn schemas_reject_unsupported_types_duplicate_ids_misplaced_bounds_and_bad_suggestions() {
    let base = serde_json::to_value(form()).unwrap();
    let mut value = base.clone();
    value["fields"][0]["kind"] = json!("password");
    assert!(specification(value).validate().is_err());
    let mut value = base.clone();
    value["fields"][1]["id"] = value["fields"][0]["id"].clone();
    assert!(specification(value).validate().is_err());
    let mut value = base.clone();
    value["fields"][0]["min"] = json!(1);
    assert!(specification(value).validate().is_err());
    let mut value = base;
    value["fields"][2]["suggested"] = json!(-1);
    assert!(specification(value).validate().is_err());
    let mut spec = choices("single_choice");
    spec.version = 2;
    assert!(spec.validate().is_err());
    spec.version = 1;
    spec.options.push(spec.options[0].clone());
    assert!(spec.validate().is_err());
}
#[test]
fn payload_bounds_and_unicode_text_limits_are_enforced() {
    let mut spec = form();
    spec.fields[0].max_length = Some(2);
    let valid = json!({"name":"你好","quantity":1,"date":"2000-02-29","express":false});
    spec.answer(&valid).unwrap();
    let mut large = valid;
    large["notes"] = json!("x".repeat(9000));
    assert!(spec.answer(&large).is_err());
    spec.description = "x".repeat(1001);
    assert!(spec.validate().is_err());
    let mut spec = form();
    while spec.fields.len() < 9 {
        spec.fields.push(spec.fields[0].clone());
    }
    assert!(spec.validate().is_err());
}
#[test]
fn model_cannot_choose_a_respondent_action_or_arbitrary_submission_url() {
    let spec = serde_json::to_value(choices("single_choice")).unwrap();
    for field in ["respondent", "workspace", "task", "approver", "url"] {
        let mut request = json!({"key":"delivery","specification":spec});
        request[field] = json!("other");
        assert!(serde_json::from_value::<Ask>(request).is_err());
    }
    let mut bad = spec.clone();
    bad["kind"] = json!("approval");
    assert!(specification(bad).validate().is_err());
    let mut bad = spec;
    bad["submit_url"] = json!("https://example.test");
    assert!(serde_json::from_value::<Specification>(bad).is_err());
}

//! Input checks for the task tools (people-and-agents-first, Phase 2): only the
//! documented fields, bounded, so a model can't forward anything else.
use serde_json::Value;

fn text(v: &serde_json::Map<String, Value>, key: &str, max: usize, required: bool) -> bool {
    match v.get(key) {
        None => !required,
        Some(value) => value
            .as_str()
            .is_some_and(|s| !s.trim().is_empty() && s.chars().count() <= max),
    }
}

fn only(v: &serde_json::Map<String, Value>, keys: &[&str]) -> bool {
    v.keys().all(|key| keys.contains(&key.as_str()))
}

fn minutes(v: &serde_json::Map<String, Value>, key: &str, max: i64, required: bool) -> bool {
    match v.get(key) {
        None => !required,
        Some(value) => value.as_i64().is_some_and(|m| (1..=max).contains(&m)),
    }
}

pub(crate) fn create(input: &Value) -> bool {
    input.as_object().is_some_and(|v| {
        only(
            v,
            &["title", "done_when", "start_in_minutes", "due_in_minutes"],
        ) && text(v, "title", 160, true)
            && text(v, "done_when", 400, false)
            && minutes(v, "start_in_minutes", 43_200, false)
            && minutes(v, "due_in_minutes", 525_600, false)
    })
}

pub(crate) fn follow_up(input: &Value) -> bool {
    input.as_object().is_some_and(|v| {
        only(v, &["task", "in_minutes", "note"])
            && text(v, "task", 64, true)
            && minutes(v, "in_minutes", 43_200, true)
            && text(v, "note", 400, false)
    })
}

pub(crate) fn attach(input: &Value) -> bool {
    input
        .as_object()
        .is_some_and(|v| only(v, &["task"]) && text(v, "task", 64, true))
}

pub(crate) fn close(input: &Value) -> bool {
    input.as_object().is_some_and(|v| {
        only(v, &["task", "outcome", "result"])
            && text(v, "task", 64, true)
            && v.get("outcome")
                .and_then(Value::as_str)
                .is_some_and(|o| ["done", "failed", "cancelled"].contains(&o))
            && text(v, "result", 1000, false)
    })
}

pub(crate) fn problem(input: &Value) -> bool {
    input.as_object().is_some_and(|v| {
        only(v, &["cause", "detail", "fix"])
            && v.get("cause").and_then(Value::as_str).is_some_and(|c| {
                [
                    "connection",
                    "permission",
                    "information",
                    "external",
                    "other",
                ]
                .contains(&c)
            })
            && text(v, "detail", 300, true)
            && text(v, "fix", 200, false)
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn task_tools_forward_only_their_bounded_fields() {
        assert!(super::create(&json!({"title":"Send quotes to Acme"})));
        assert!(super::create(
            &json!({"title":"Send quotes","done_when":"Acme has both quotes"})
        ));
        assert!(!super::create(&json!({"title":""})));
        assert!(!super::create(&json!({"title":"x".repeat(161)})));
        assert!(!super::create(&json!({"title":"Send","owner":"someone"})));
        assert!(super::attach(&json!({"task":"t-1"})));
        assert!(!super::attach(&json!({"task":"t-1","agent":"other"})));
        assert!(super::close(
            &json!({"task":"t-1","outcome":"done","result":"Sent both quotes"})
        ));
        assert!(!super::close(&json!({"task":"t-1","outcome":"not_task"})));
        assert!(!super::close(&json!({"task":"t-1"})));
        assert!(super::create(
            &json!({"title":"Call back","start_in_minutes":1440,"due_in_minutes":2880})
        ));
        assert!(!super::create(
            &json!({"title":"Call back","start_in_minutes":0})
        ));
        assert!(super::follow_up(
            &json!({"task":"t-1","in_minutes":60,"note":"Check the courier"})
        ));
        assert!(!super::follow_up(&json!({"task":"t-1"})));
        assert!(!super::follow_up(&json!({"task":"t-1","in_minutes":50000})));
        assert!(super::problem(
            &json!({"cause":"connection","detail":"Shopify needs reconnecting","fix":"Reconnect Shopify in Settings"})
        ));
        assert!(!super::problem(&json!({"cause":"bug","detail":"x"})));
        assert!(!super::problem(&json!({"cause":"other"})));
    }
}

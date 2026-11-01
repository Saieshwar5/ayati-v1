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

pub(crate) fn create(input: &Value) -> bool {
    input.as_object().is_some_and(|v| {
        only(v, &["title", "done_when"])
            && text(v, "title", 160, true)
            && text(v, "done_when", 400, false)
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
    }
}

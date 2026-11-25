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

/// A calendar rule: only its documented fields, each bounded; the controller checks
/// the rest (which fields fit which unit).
fn rule(value: Option<&Value>) -> bool {
    let Some(value) = value else { return true };
    value.as_object().is_some_and(|r| {
        only(
            r,
            &[
                "unit",
                "every",
                "at",
                "days",
                "day_of_month",
                "from",
                "to",
                "working_days",
            ],
        ) && r
            .get("unit")
            .and_then(Value::as_str)
            .is_some_and(|u| ["minute", "hour", "day", "week", "month"].contains(&u))
            && minutes(r, "every", 1440, false)
            && ["at", "from", "to"]
                .iter()
                .all(|key| text(r, key, 5, false))
            && r.get("days").is_none_or(|d| {
                d.as_array().is_some_and(|days| {
                    (1..=7).contains(&days.len())
                        && days
                            .iter()
                            .all(|d| d.as_i64().is_some_and(|d| (1..=7).contains(&d)))
                })
            })
            && r.get("day_of_month")
                .is_none_or(|d| d.as_i64().is_some_and(|d| d == -1 || (1..=31).contains(&d)))
            && r.get("working_days").is_none_or(Value::is_boolean)
    })
}

/// When a task runs: a rule, a local first run, a local last day and a run count.
fn timing(v: &serde_json::Map<String, Value>) -> bool {
    rule(v.get("repeat"))
        && text(v, "first_at", 16, false)
        && text(v, "until", 10, false)
        && minutes(v, "max_runs", 1000, false)
        && text(v, "instructions", 4000, false)
}

const TIMING: [&str; 5] = ["repeat", "first_at", "until", "max_runs", "instructions"];

pub(crate) fn create(input: &Value) -> bool {
    input.as_object().is_some_and(|v| {
        only(
            v,
            &[
                &["title", "done_when", "start_in_minutes", "due_in_minutes"][..],
                &TIMING,
            ]
            .concat(),
        ) && text(v, "title", 160, true)
            && text(v, "done_when", 400, false)
            && minutes(v, "start_in_minutes", 43_200, false)
            && minutes(v, "due_in_minutes", 525_600, false)
            && timing(v)
    })
}

pub(crate) fn change(input: &Value) -> bool {
    input.as_object().is_some_and(|v| {
        only(
            v,
            &[&["task", "title", "done_when", "paused"][..], &TIMING].concat(),
        ) && text(v, "task", 64, true)
            && text(v, "title", 160, false)
            && text(v, "done_when", 400, false)
            && v.get("paused").is_none_or(Value::is_boolean)
            && timing(v)
            && v.len() > 1
    })
}

pub(crate) fn note(input: &Value) -> bool {
    input.as_object().is_some_and(|v| {
        only(v, &["task", "note"]) && text(v, "task", 64, true) && text(v, "note", 400, true)
    })
}

pub(crate) fn continue_later(input: &Value) -> bool {
    input.as_object().is_some_and(|v| {
        only(v, &["task", "done", "next"])
            && text(v, "task", 64, true)
            && text(v, "done", 400, true)
            && text(v, "next", 400, true)
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

pub(crate) fn goal(input: &Value) -> bool {
    input.as_object().is_some_and(|v| {
        only(v, &["goal", "current", "risk"])
            && text(v, "goal", 64, true)
            && v.get("current")
                .and_then(Value::as_f64)
                .is_some_and(|c| c >= 0.0)
            && text(v, "risk", 280, false)
    })
}

pub(crate) fn review(input: &Value) -> bool {
    input.as_object().is_some_and(|v| {
        only(v, &["done", "went_wrong", "learned", "suggestion"])
            && text(v, "done", 1000, true)
            && text(v, "went_wrong", 1000, true)
            && text(v, "learned", 1000, true)
            && text(v, "suggestion", 1000, false)
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
        assert!(super::create(
            &json!({"title":"Chase invoice 311","done_when":"Invoice 311 is paid",
            "repeat":{"unit":"day","every":2,"at":"10:00","working_days":true},"max_runs":5})
        ));
        assert!(super::create(
            &json!({"title":"Morning brief","repeat":{"unit":"week","days":[1,2,3,4,5],"at":"09:00"},
            "first_at":"2026-11-25 09:00","until":"2026-12-31"})
        ));
        assert!(!super::create(
            &json!({"title":"x","repeat":{"unit":"second"}})
        ));
        assert!(super::create(
            &json!({"title":"Check stock","repeat":{"unit":"minute","every":90},"instructions":"List items below reorder level"})
        ));
        assert!(!super::create(
            &json!({"title":"x","instructions":"y".repeat(4001)})
        ));
        assert!(!super::create(
            &json!({"title":"x","repeat":{"unit":"week","days":[0]}})
        ));
        assert!(!super::create(
            &json!({"title":"x","repeat":{"unit":"day","cron":"* *"}})
        ));
        assert!(!super::create(&json!({"title":"x","max_runs":0})));
        assert!(super::change(
            &json!({"task":"t-1","repeat":{"unit":"day","every":3}})
        ));
        assert!(super::change(&json!({"task":"t-1","paused":true})));
        assert!(!super::change(&json!({"task":"t-1"})), "say what to change");
        assert!(!super::change(&json!({"task":"t-1","agent":"other"})));
        assert!(super::note(&json!({"task":"t-1","note":"Sent reminder 2"})));
        assert!(!super::note(&json!({"task":"t-1","note":""})));
        assert!(super::continue_later(
            &json!({"task":"t-1","done":"Read 3 of 9 files","next":"Read the rest"})
        ));
        assert!(!super::continue_later(&json!({"task":"t-1","done":"x"})));
        assert!(super::goal(
            &json!({"goal":"g-1","current":42.5,"risk":"Two suppliers are late"})
        ));
        assert!(!super::goal(&json!({"goal":"g-1","current":-1})));
        assert!(super::review(
            &json!({"done":"Sent 40 quotes","went_wrong":"Two failed on Zoho","learned":"Check the token first"})
        ));
        assert!(!super::review(&json!({"done":"x"})));
    }
}

//! The shape of an agent's proposed company rule; the controller checks it again
//! against the workspace's actions before saving it as a proposal.
use serde_json::{Value, json};

pub(crate) fn tool() -> Value {
    json!({"name":"propose_rule","description":"When a person asks for a company rule (a limit or condition every agent must follow, such as \"refunds over 2000 need my approval\" or \"no customer messages after 9 pm\"), draft it once. words: their words. Either operation (and connection) for one action from operations, or meaning (money, sends_outside, change, delete) for every action of that kind. kind: field (compare one field), daily_total (today's total of a number field), time (value {from:\"HH:MM\",to:\"HH:MM\",days:[1-7]}, field empty, comparator =) or read_first (source {operation,match} must have been read first; field empty, comparator =, value true). source {operation,match,path} takes the value from a read you made earlier in this task. effect: ask or block. An admin reviews it; it isn't in force until they turn it on. Never propose a rule to loosen or remove one.","inputSchema":{"type":"object","properties":{
        "words":{"type":"string","maxLength":280},"operation":{"type":"string","maxLength":100},"connection":{"type":"string","maxLength":100},
        "meaning":{"type":"string","enum":["change","money","sends_outside","delete"]},
        "kind":{"type":"string","enum":["field","daily_total","time","read_first"]},
        "source":{"type":"object"},"field":{"type":"string","maxLength":80},
        "comparator":{"type":"string","enum":[">",">=","<","<=","=","!="]},
        "value":{},"effect":{"type":"string","enum":["ask","block"]}},
        "required":["words","kind","comparator","value","effect"],"additionalProperties":false}})
}

pub(crate) fn proposal(input: &Value) -> bool {
    let Some(v) = input.as_object() else {
        return false;
    };
    let text = |key: &str, max: usize, required: bool| {
        v.get(key).map_or(!required, |s| {
            s.as_str()
                .is_some_and(|s| !s.trim().is_empty() && s.chars().count() <= max)
        })
    };
    let one_of = |key: &str, allowed: &[&str], required: bool| {
        v.get(key).map_or(!required, |s| {
            s.as_str().is_some_and(|s| allowed.contains(&s))
        })
    };
    v.keys().all(|k| {
        [
            "words",
            "operation",
            "connection",
            "meaning",
            "kind",
            "source",
            "field",
            "comparator",
            "value",
            "effect",
        ]
        .contains(&k.as_str())
    }) && text("words", 280, true)
        && text("operation", 100, false)
        && text("connection", 100, false)
        && v.get("field")
            .is_none_or(|f| f.as_str().is_some_and(|f| f.chars().count() <= 80))
        && one_of(
            "meaning",
            &["change", "money", "sends_outside", "delete"],
            false,
        )
        && one_of(
            "kind",
            &["field", "daily_total", "time", "read_first"],
            true,
        )
        && one_of("comparator", &[">", ">=", "<", "<=", "=", "!="], true)
        && one_of("effect", &["ask", "block"], true)
        && v.get("value").is_some_and(|value| {
            value.is_number()
                || value.is_boolean()
                || value.as_str().is_some_and(|s| s.chars().count() <= 200)
                || value.is_object()
        })
        && v.get("source").is_none_or(Value::is_object)
        && serde_json::to_vec(input).is_ok_and(|b| b.len() <= 2000)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    #[test]
    fn proposals_have_words_a_kind_and_an_effect() {
        assert!(super::proposal(
            &json!({"words":"Refunds over 2000 need my approval","operation":"refund.create","kind":"field","field":"amount","comparator":">","value":2000,"effect":"ask"})
        ));
        assert!(!super::proposal(
            &json!({"operation":"refund.create","kind":"field","field":"amount","comparator":">","value":2000,"effect":"ask"})
        ));
        assert!(!super::proposal(
            &json!({"words":"x","kind":"field","comparator":">","value":2000,"effect":"allow"})
        ));
        assert!(!super::proposal(
            &json!({"words":"x","kind":"field","comparator":">","value":[1],"effect":"ask"})
        ));
        assert!(!super::proposal(
            &json!({"words":"x","kind":"field","comparator":">","value":1,"effect":"ask","enabled":true})
        ));
    }
}

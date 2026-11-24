//! Memory tools (schema 142). The controller decides from the running task
//! which shelves a request reads and writes; these arguments carry only the
//! words, never a person, room or agent.
use serde_json::{Value, json};

const KINDS: [&str; 8] = [
    "customer",
    "system",
    "people",
    "decision",
    "practice",
    "preference",
    "status",
    "term",
];

fn text(fields: &serde_json::Map<String, Value>, key: &str, max: usize) -> bool {
    fields
        .get(key)
        .and_then(Value::as_str)
        .is_some_and(|v| !v.trim().is_empty() && v.len() <= max)
}

fn optional(fields: &serde_json::Map<String, Value>, key: &str, max: usize) -> bool {
    fields
        .get(key)
        .is_none_or(|v| v.is_null() || v.as_str().is_some_and(|v| v.len() <= max))
}

pub fn observed(name: &str, input: &Value) -> bool {
    let Some(fields) = input.as_object() else {
        return false;
    };
    match name {
        "remember" => {
            fields.keys().all(|key| {
                ["text", "kind", "topic", "until", "for", "replaces"].contains(&key.as_str())
            }) && text(fields, "text", 1200)
                && fields
                    .get("kind")
                    .and_then(Value::as_str)
                    .is_some_and(|k| KINDS.contains(&k))
                && optional(fields, "topic", 320)
                && optional(fields, "until", 10)
                && optional(fields, "replaces", 64)
                && fields.get("for").is_none_or(|v| {
                    v.is_null()
                        || ["here", "my_job", "workspace"].contains(&v.as_str().unwrap_or(""))
                })
        }
        "end_memory" => {
            fields
                .keys()
                .all(|key| ["memory", "reason"].contains(&key.as_str()))
                && text(fields, "memory", 64)
                && optional(fields, "reason", 800)
        }
        _ => false,
    }
}

pub fn tools() -> Vec<Value> {
    vec![
        json!({"name":"remember","description":"Keep one short, lasting note you learned while working, so you and other agents know it next time. Keep what will still matter in a month: how a customer pays, who approves what, what a word means here, how a system behaves, how this person or room wants things. Never passwords, secrets, one-off details, live numbers (stock, balances, order status: read those from the app) or instructions. for: here (default: only this conversation, or only this person in their own chat), my_job (what you learned about your own work, for every conversation) or workspace (a fact about the business any agent needs). A note filed wider than here from a private, outside or background place waits for a person. kind: customer, system, people, decision, practice, preference, status or term. topic names what it is about (Acme Traders, Invoice system). until: a date YYYY-MM-DD after which it no longer holds. To correct a note, set replaces to its name.","inputSchema":{"type":"object","properties":{
            "text":{"type":"string","maxLength":300},
            "kind":{"type":"string","enum":KINDS},
            "topic":{"type":"string","maxLength":80},
            "until":{"type":"string","maxLength":10},
            "for":{"type":"string","enum":["here","my_job","workspace"]},
            "replaces":{"type":"string","maxLength":64}
        },"required":["text","kind"],"additionalProperties":false}}),
        json!({"name":"end_memory","description":"End one note that is no longer true or that someone asked you to forget: memory is its name as shown in brackets, such as 3f9a12bc. A person can bring it back. Some notes only a person can end; then remember the correction with replaces instead.","inputSchema":{"type":"object","properties":{
            "memory":{"type":"string","maxLength":64},
            "reason":{"type":"string","maxLength":200}
        },"required":["memory"],"additionalProperties":false}}),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn memory_tools_never_carry_a_person_room_or_agent() {
        let note = json!({"text":"Acme pays 45 days after invoice.","kind":"customer","topic":"Acme Traders","for":"workspace"});
        assert!(observed("remember", &note));
        for key in ["agent", "chat", "user_id", "person", "shelf", "scope"] {
            let mut forged = note.clone();
            forged[key] = json!("x");
            assert!(!observed("remember", &forged), "{key}");
        }
        let mut wider = note.clone();
        wider["for"] = json!("everyone");
        assert!(!observed("remember", &wider));
        let mut kind = note.clone();
        kind["kind"] = json!("rule");
        assert!(!observed("remember", &kind));
        assert!(observed("end_memory", &json!({"memory":"3f9a12bc"})));
        assert!(!observed(
            "end_memory",
            &json!({"memory":"3f9a12bc","chat":"c1"})
        ));
        assert_eq!(tools().len(), 2);
    }
}

//! A person's assistant drafts an agent for them (schema 139): in New agent, on
//! an agent's page, or when asked in its own chat with them. It hands back one
//! drafted part per call. The controller decides whose assistant this is and
//! whether it may draft here; nothing here saves anything to an agent.
use serde_json::{Value, json};

pub const PARTS: [&str; 8] = [
    "name",
    "ground_rules",
    "skill",
    "remove_skill",
    "reuse_skill",
    "needs_app",
    "job_title",
    "used",
];

pub fn observed(input: &Value) -> bool {
    let Some(fields) = input.as_object() else {
        return false;
    };
    let text = |key: &str, max: usize| {
        fields.get(key).is_none_or(|value| {
            value
                .as_str()
                .is_some_and(|v| !v.trim().is_empty() && v.chars().count() <= max)
        })
    };
    fields
        .get("part")
        .and_then(Value::as_str)
        .is_some_and(|part| PARTS.contains(&part))
        && fields
            .keys()
            .all(|key| ["part", "name", "description", "text"].contains(&key.as_str()))
        && text("name", 120)
        && text("description", 1024)
        && text("text", 6000)
}

pub fn tool() -> Value {
    json!({
        "name":"write_agent_setup",
        "description":"Only for a person's own assistant: when they ask you in your chat with them for a new agent, or for help setting one up in New agent or on an agent's page. Hand back one drafted part per call: name (text: a new agent's name, only when asked in chat), ground_rules (text), skill (name, description starting \"Use when\", text: the numbered steps), remove_skill (name), reuse_skill (name of a workspace skill), needs_app (name of an app the job needs), job_title (text) or used (text: one thing from your memory or the Library you relied on). Nothing is saved: your person reviews every part and creates or changes the agent themselves.",
        "inputSchema":{"type":"object","properties":{
            "part":{"type":"string","enum":PARTS},
            "name":{"type":"string","maxLength":120},
            "description":{"type":"string","maxLength":1024},
            "text":{"type":"string","maxLength":6000}
        },"required":["part"],"additionalProperties":false}
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_setup_part_carries_only_its_words() {
        assert!(observed(
            &json!({"part":"ground_rules","text":"You are Iris."})
        ));
        assert!(observed(
            &json!({"part":"skill","name":"order-status","description":"Use when a customer asks about an order.","text":"1. Look up the order."})
        ));
        assert!(observed(&json!({"part":"needs_app","name":"Shopify"})));
        assert!(observed(&json!({"part":"name","text":"Iris"})));
        // The agent, the person and what is saved are the controller's to decide.
        for key in ["agent", "owner", "save", "audience"] {
            let mut forged = json!({"part":"ground_rules","text":"You are Iris."});
            forged[key] = json!("x");
            assert!(!observed(&forged), "{key}");
        }
        assert!(!observed(&json!({"part":"controls","text":"Allow all"})));
        assert!(!observed(&json!({"text":"You are Iris."})));
        assert!(!observed(&json!({"part":"ground_rules","text":" "})));
        assert!(!observed(
            &json!({"part":"ground_rules","text":"x".repeat(6001)})
        ));
    }
}

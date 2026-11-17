//! An assistant's threads (schema 128): suggest one to its person, answer in a
//! running one, finish it. The controller decides whose assistant this is,
//! whether the thread runs and who the message is from; these arguments only
//! name the thread and carry the words.
use serde_json::{Value, json};

fn text(fields: &serde_json::Map<String, Value>, key: &str, max: usize) -> bool {
    fields
        .get(key)
        .and_then(Value::as_str)
        .is_some_and(|v| !v.trim().is_empty() && v.len() <= max)
}

pub fn observed(name: &str, input: &Value) -> bool {
    let Some(fields) = input.as_object() else {
        return false;
    };
    let wanted: &[(&str, usize)] = match name {
        "suggest_thread" => &[("to", 120), ("title", 480), ("opening", 4000)],
        "thread_reply" => &[("thread", 64), ("key", 64), ("text", 4000)],
        "finish_thread" => &[("thread", 64), ("summary", 1600)],
        "remember_about_person" => &[("kind", 8), ("text", 1200)],
        _ => return false,
    };
    fields.len() == wanted.len() && wanted.iter().all(|(key, max)| text(fields, key, *max))
}

pub fn tools() -> Vec<Value> {
    vec![
        json!({"name":"suggest_thread","description":"Only for a person's own assistant. When a matter needs several messages back and forth with one agent or one person (chasing a leave request with HR Desk, arranging a meeting time with a colleague), suggest a thread: to (their exact name or ID), a short title, and the opening message written as your person would write it. Nothing is sent: your person sees the suggestion and starts it. After that you answer there for them until they stop it. For a single question, just tell your person what to ask.","inputSchema":{"type":"object","properties":{
            "to":{"type":"string","maxLength":120},
            "title":{"type":"string","maxLength":120},
            "opening":{"type":"string","maxLength":4000}
        },"required":["to","title","opening"],"additionalProperties":false}}),
        json!({"name":"thread_reply","description":"Only in a running thread your person started. Send one message there: it is posted as your person's own message, so write as them. Use a stable key for exact retries. If you need your person's decision or information they did not agree to share, do not send; say what you need in your reply instead.","inputSchema":{"type":"object","properties":{
            "thread":{"type":"string","maxLength":64},
            "key":{"type":"string","maxLength":64},
            "text":{"type":"string","maxLength":4000}
        },"required":["thread","key","text"],"additionalProperties":false}}),
        json!({"name":"finish_thread","description":"When a thread's matter is settled, stop it with one line on how it ended. Your person can resume it.","inputSchema":{"type":"object","properties":{
            "thread":{"type":"string","maxLength":64},
            "summary":{"type":"string","maxLength":400}
        },"required":["thread","summary"],"additionalProperties":false}}),
        json!({"name":"remember_about_person","description":"Only for a person's own assistant. Keep one short, lasting line about your person: kind about (their role, people, priorities, preferences) or style (how they write: tone, length, sign-off). They can read and edit these notes. Never keep passwords, secrets or one-off details.","inputSchema":{"type":"object","properties":{
            "kind":{"type":"string","enum":["about","style"]},
            "text":{"type":"string","maxLength":300}
        },"required":["kind","text"],"additionalProperties":false}}),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn thread_tools_carry_only_the_thread_and_the_words() {
        let reply = json!({"thread":"t1","key":"k1","text":"Friday works for me."});
        assert!(observed("thread_reply", &reply));
        for key in ["chat", "as", "author", "agent"] {
            let mut forged = reply.clone();
            forged[key] = json!("x");
            assert!(!observed("thread_reply", &forged), "{key}");
        }
        assert!(!observed(
            "thread_reply",
            &json!({"thread":"t1","key":"k1","text":" "})
        ));
        assert!(observed(
            "suggest_thread",
            &json!({"to":"HR Desk","title":"Leave in December","opening":"Hi, I'd like leave from 22 to 26 December."})
        ));
        assert!(!observed(
            "suggest_thread",
            &json!({"to":"HR Desk","title":"Leave"})
        ));
        assert!(observed(
            "finish_thread",
            &json!({"thread":"t1","summary":"Leave approved."})
        ));
        assert!(observed(
            "remember_about_person",
            &json!({"kind":"style","text":"Short and friendly."})
        ));
        assert_eq!(tools().len(), 4);
    }
}

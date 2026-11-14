//! Events from a connected system (C2 and C5 of the connected systems plan,
//! D249). A system sends one web call with a fixed envelope:
//!
//! ```json
//! {"id":"evt-5001","type":"exam.result_published","subject":"stu-104",
//!  "time":"2026-11-13T09:00:00Z","data":{"exam":"Maths term 1"}}
//! ```
//!
//! A field is in the envelope only when Cuckoo itself acts on it; everything
//! only the agent needs goes in `data`. Unknown envelope fields are ignored.
//! `data` may be nested JSON (C5); the fields an automation chose reach the
//! agent whole, within 16 KiB.
use super::catalog::{Source, Subscribed};
use anyhow::Result;
use serde_json::{Map, Value, json};

/// Most bytes of the chosen `data` fields that reach the agent.
pub(crate) const DATA_BYTES: usize = 16 * 1024;

/// The envelope after its checks.
#[derive(Debug, PartialEq)]
pub(crate) struct Envelope {
    pub id: String,
    pub kind: String,
    pub subject: Option<String>,
    pub time: Option<String>,
    pub data: Value,
}

/// Why an envelope was refused, naming the field (answered with 422).
#[derive(Debug)]
pub(crate) struct Invalid(pub String);
impl std::fmt::Display for Invalid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Invalid {}

fn invalid(message: &str) -> anyhow::Error {
    Invalid(message.to_owned()).into()
}

/// An event type: lowercase letters, digits, dots and underscores, starting
/// with a letter, up to 64 characters.
pub(crate) fn valid_type(kind: &str) -> bool {
    !kind.is_empty()
        && kind.len() <= 64
        && kind.as_bytes()[0].is_ascii_lowercase()
        && kind
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'_')
}

/// Checks the fixed envelope of one event.
pub(crate) fn envelope(body: &Value) -> Result<Envelope> {
    let object = body
        .as_object()
        .ok_or_else(|| invalid("The event must be a JSON object."))?;
    let id = object
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| crate::identity::identifier(id).is_ok())
        .ok_or_else(|| invalid("\u{201c}id\u{201d} must be up to 128 letters, digits, - or _."))?;
    let kind = object
        .get("type")
        .and_then(Value::as_str)
        .filter(|t| valid_type(t))
        .ok_or_else(|| {
            invalid("\u{201c}type\u{201d} must be up to 64 lowercase letters, digits, dots or underscores, starting with a letter.")
        })?;
    let text = |key: &str, max: usize| -> Result<Option<String>> {
        match object.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s))
                if !s.is_empty() && s.len() <= max && !s.chars().any(char::is_control) =>
            {
                Ok(Some(s.clone()))
            }
            Some(_) => Err(invalid(&format!(
                "\u{201c}{key}\u{201d} must be text of up to {max} characters."
            ))),
        }
    };
    let subject = text("subject", 128)?;
    let time = text("time", 64)?;
    let data = match object.get("data") {
        None | Some(Value::Null) => json!({}),
        Some(Value::Object(o)) => Value::Object(o.clone()),
        Some(_) => return Err(invalid("\u{201c}data\u{201d} must be a JSON object.")),
    };
    Ok(Envelope {
        id: id.to_owned(),
        kind: kind.to_owned(),
        subject,
        time,
        data,
    })
}

/// The top-level field names of `data`, for Recent events and the automation
/// form. Values are never shown there.
pub(crate) fn field_names(data: &Value) -> Vec<String> {
    let mut names: Vec<String> = data
        .as_object()
        .map(|o| o.keys().take(50).cloned().collect())
        .unwrap_or_default();
    names.sort();
    names
}

/// What the agent receives from one event for an automation: its type and
/// subject under `$type` and `$subject` (never a field name, since fields are
/// identifiers), and each chosen field whole, nested values included. A
/// missing field is null. The chosen fields together stay within 16 KiB, or
/// the event is refused with the largest field named.
pub(crate) fn project(source: &Source, s: &Subscribed, id: &str, event: &Value) -> Result<Value> {
    let envelope = envelope(event)?;
    anyhow::ensure!(
        envelope.id == id && envelope.kind == s.event,
        "Event identity does not match its automation"
    );
    let mut projected = Map::new();
    for field in &source.fields {
        projected.insert(
            field.clone(),
            envelope.data.get(field).cloned().unwrap_or(Value::Null),
        );
    }
    let size = serde_json::to_vec(&projected)?.len();
    if size > DATA_BYTES {
        let largest = projected
            .iter()
            .max_by_key(|(_, v)| serde_json::to_vec(v).map_or(0, |b| b.len()))
            .map(|(k, _)| k.clone())
            .unwrap_or_default();
        return Err(Invalid(format!(
            "The chosen fields of \u{201c}data\u{201d} are {size} bytes; at most 16,384 reach the agent. The largest is \u{201c}{largest}\u{201d}."
        ))
        .into());
    }
    projected.insert("$type".into(), json!(envelope.kind));
    projected.insert("$subject".into(), json!(envelope.subject));
    Ok(Value::Object(projected))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn source(fields: &[&str]) -> (Source, Subscribed) {
        let map: BTreeMap<String, String> = fields
            .iter()
            .map(|f| ((*f).to_owned(), (*f).to_owned()))
            .collect();
        let subscribed = Subscribed {
            event: "exam.result_published".into(),
            map,
            active: true,
        };
        let source = Source {
            id: "11111111-1111-4111-8111-111111111111".into(),
            workspace: "22222222-2222-4222-8222-222222222222".into(),
            connection: "33333333-3333-4333-8333-333333333333".into(),
            connection_fingerprint: "a".repeat(43),
            name: "StudyHub: exam.result_published".into(),
            provider: "system".into(),
            user_id: String::new(),
            connected_account_id: String::new(),
            auth_config_id: String::new(),
            trigger_id: "44444444-4444-4444-8444-444444444444".into(),
            trigger_slug: "exam.result_published".into(),
            read_operation: "get_marks".into(),
            fields: fields.iter().map(|f| (*f).to_owned()).collect(),
            secret_file: String::new(),
            subscribed: Some(subscribed.clone()),
        };
        (source, subscribed)
    }

    #[test]
    fn the_envelope_is_fixed_and_unknown_fields_are_ignored() {
        let e = envelope(
            &json!({"id":"evt-5001","type":"exam.result_published","subject":"stu-104",
            "time":"2026-11-13T09:00:00Z","data":{"exam":"Maths term 1"},"extra":"ignored"}),
        )
        .unwrap();
        assert_eq!(e.kind, "exam.result_published");
        assert_eq!(e.subject.as_deref(), Some("stu-104"));
        assert_eq!(
            envelope(&json!({"id":"e1","type":"a"})).unwrap().data,
            json!({})
        );
        for (bad, field) in [
            (json!({"type":"a"}), "id"),
            (json!({"id":"has space","type":"a"}), "id"),
            (json!({"id":"e","type":"Exam"}), "type"),
            (json!({"id":"e","type":"9x"}), "type"),
            (json!({"id":"e","type":"a","subject":7}), "subject"),
            (json!({"id":"e","type":"a","data":[1]}), "data"),
        ] {
            let error = envelope(&bad).unwrap_err().to_string();
            assert!(error.contains(field), "{bad} → {error}");
        }
    }

    #[test]
    fn chosen_fields_reach_the_agent_whole_even_when_nested() {
        let (source, s) = source(&["exam", "scores", "missing"]);
        let event = json!({"id":"evt-1","type":"exam.result_published","subject":"stu-104",
            "data":{"exam":"Maths","scores":{"algebra":41,"geometry":[38,52]},"private":"not chosen"}});
        let projected = project(&source, &s, "evt-1", &event).unwrap();
        assert_eq!(projected["scores"]["geometry"][1], 52);
        assert!(projected["missing"].is_null());
        assert!(projected.get("private").is_none());
        assert_eq!(projected["$type"], "exam.result_published");
        assert_eq!(projected["$subject"], "stu-104");
        assert!(
            project(&source, &s, "evt-2", &event).is_err(),
            "the id must match"
        );
    }

    #[test]
    fn oversized_data_is_refused_with_the_largest_field_named() {
        let (source, s) = source(&["exam", "essay"]);
        let event = json!({"id":"evt-1","type":"exam.result_published",
            "data":{"exam":"Maths","essay":"x".repeat(20000)}});
        let error = project(&source, &s, "evt-1", &event)
            .unwrap_err()
            .to_string();
        assert!(error.contains("\u{201c}essay\u{201d}"), "{error}");
    }
}

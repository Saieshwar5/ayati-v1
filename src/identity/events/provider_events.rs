//! Provider deliveries in Cuckoo's terms (schema 109). A verified delivery for
//! a controller-owned subscription becomes one normalized event; its hash is the
//! receipt's content and only the catalog's mapped fields are ever kept.
use super::catalog::{Source, Subscribed};
use crate::identity::connectors::{Delivery, DeliveryKind};
use anyhow::{Context, Result, ensure};
use serde_json::{Map, Value, json};

/// The longest text kept from one field; longer text is cut, not refused, so a
/// long email still starts its trigger. The agent can read the rest itself.
const TEXT: usize = 2000;
/// The projected fields together stay within the receipt's 6 KiB.
const TOTAL: usize = 6144;

/// The event saved for a verified app event, or `None` for other deliveries.
/// A retry with the same id and content is a duplicate; changed content is refused.
pub(crate) fn normalized(delivery: &Delivery) -> Option<Value> {
    let DeliveryKind::Event {
        subscription,
        trigger,
        data,
    } = &delivery.kind
    else {
        return None;
    };
    Some(
        json!({"id":delivery.event_id,"type":"provider.event","user":delivery.user,
        "account":delivery.account,"subscription":subscription,"trigger":trigger,"data":data}),
    )
}

/// Checks the event belongs to this subscription and keeps the mapped fields
/// under Cuckoo's names. A delivery that names a user or an account must name
/// the subscription's; the subscription and trigger must always match.
pub(super) fn project(source: &Source, s: &Subscribed, id: &str, event: &Value) -> Result<Value> {
    ensure!(
        event["id"] == id && event["type"] == "provider.event",
        "Unsupported event envelope"
    );
    let named = |key: &str, expected: &str| event[key].is_null() || event[key] == expected;
    ensure!(
        event["subscription"] == source.trigger_id.as_str()
            && event["trigger"] == source.trigger_slug.as_str()
            && named("user", &source.user_id)
            && named("account", &source.connected_account_id),
        "Event identity does not match its subscription"
    );
    let data = event["data"]
        .as_object()
        .context("Event data must be an object")?;
    let mut projected = Map::new();
    for (cuckoo, field) in &s.map {
        projected.insert(cuckoo.clone(), scalar(data.get(field)));
    }
    fit(&mut projected)?;
    Ok(Value::Object(projected))
}

/// A plain value, as Cuckoo keeps it: missing fields, lists and objects become
/// null; text is cut to [`TEXT`] bytes.
fn scalar(value: Option<&Value>) -> Value {
    match value {
        Some(Value::Bool(b)) => json!(b),
        Some(Value::Number(n))
            if n.as_f64()
                .is_some_and(|n| n.is_finite() && n.abs() <= 9_007_199_254_740_991.0) =>
        {
            Value::Number(n.clone())
        }
        Some(Value::String(text)) => json!(cut(&text.replace('\0', ""), TEXT)),
        _ => Value::Null,
    }
}

fn cut(text: &str, bytes: usize) -> String {
    let mut end = bytes.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

/// Shortens the longest text until everything fits in [`TOTAL`] bytes.
fn fit(fields: &mut Map<String, Value>) -> Result<()> {
    for _ in 0..32 {
        let size = serde_json::to_vec(&*fields)?.len();
        if size <= TOTAL {
            return Ok(());
        }
        let longest = fields
            .iter()
            .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.len())))
            .max_by_key(|(_, len)| *len)
            .context("Projected event data exceeds 6 KiB")?;
        let text = fields[&longest.0].as_str().unwrap_or_default().to_owned();
        let keep = longest.1.saturating_sub(size - TOTAL + 16);
        fields.insert(longest.0, json!(cut(&text, keep)));
    }
    anyhow::bail!("Projected event data exceeds 6 KiB")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> Source {
        let map: std::collections::BTreeMap<String, String> =
            [("from", "sender"), ("text", "message_text")]
                .into_iter()
                .map(|(a, b)| (a.to_owned(), b.to_owned()))
                .collect();
        Source {
            id: "11111111-1111-4111-8111-111111111111".into(),
            workspace: "22222222-2222-4222-8222-222222222222".into(),
            connection: "33333333-3333-4333-8333-333333333333".into(),
            connection_fingerprint: "a".repeat(43),
            name: "New email · Gmail".into(),
            provider: "composio".into(),
            user_id: "cuckoo-22222222-2222-4222-8222-222222222222".into(),
            connected_account_id: "ca_1".into(),
            auth_config_id: String::new(),
            trigger_id: "ti_1".into(),
            trigger_slug: "GMAIL_NEW_GMAIL_MESSAGE".into(),
            read_operation: "gmail.find_emails".into(),
            fields: map.keys().cloned().collect(),
            secret_file: String::new(),
            subscribed: Some(Subscribed {
                event: "gmail.new_email".into(),
                map,
                active: true,
            }),
        }
    }

    fn delivery(data: Value) -> Delivery {
        Delivery {
            event_id: "msg_1".into(),
            user: Some("cuckoo-22222222-2222-4222-8222-222222222222".into()),
            account: Some("ca_1".into()),
            kind: DeliveryKind::Event {
                subscription: "ti_1".into(),
                trigger: "GMAIL_NEW_GMAIL_MESSAGE".into(),
                data,
            },
        }
    }

    #[test]
    fn a_subscription_keeps_only_mapped_plain_fields_under_cuckoo_names() {
        let s = source();
        s.validate().unwrap();
        let event = normalized(&delivery(
            json!({"sender":"a@example.test","message_text":"Hi","payload":{"raw":"NEVER"}}),
        ))
        .unwrap();
        assert_eq!(
            s.project("msg_1", &event).unwrap(),
            json!({"from":"a@example.test","text":"Hi"})
        );
        // Lists, objects and missing fields become null; long text is cut.
        let event = normalized(&delivery(
            json!({"sender":["x"],"message_text":"é".repeat(5000)}),
        ))
        .unwrap();
        let projected = s.project("msg_1", &event).unwrap();
        assert!(projected["from"].is_null());
        assert!(projected["text"].as_str().unwrap().len() <= TEXT);
        // The subscription, trigger, user and account must be this source's.
        for (key, forged) in [
            ("subscription", "ti_2"),
            ("trigger", "OTHER"),
            ("user", "cuckoo-other"),
            ("account", "ca_2"),
        ] {
            let mut e = normalized(&delivery(json!({}))).unwrap();
            e[key] = json!(forged);
            assert!(s.project("msg_1", &e).is_err(), "{key}");
        }
        assert!(s.project("msg_2", &event).is_err());
        assert!(!s.public().to_string().contains("GMAIL_NEW"));
    }

    #[test]
    fn many_long_fields_are_shortened_to_fit_the_receipt() {
        let mut fields = Map::new();
        for n in 0..8 {
            fields.insert(format!("f{n}"), json!("\"".repeat(TEXT)));
        }
        fit(&mut fields).unwrap();
        assert!(serde_json::to_vec(&fields).unwrap().len() <= TOTAL);
    }
}

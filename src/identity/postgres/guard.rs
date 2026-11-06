//! The Guard's company rules (core-agent CA1): one condition on one reviewed action,
//! or on every action of one meaning, checked inside the action's own transaction.
//! A rule can only make an action stricter (ask first or block); it never allows
//! what the action rules block.
use super::*;
use crate::identity::control::model::Operation;
use serde_json::json;

#[cfg(test)]
#[path = "guard_fact_tests.rs"]
mod fact_tests;
#[cfg(test)]
#[path = "guard_meaning_tests.rs"]
mod meaning_tests;
#[path = "guard_migration.rs"]
pub(in crate::identity::postgres) mod migration;
#[path = "guard_proposals.rs"]
pub(crate) mod proposals;
#[path = "guard_store.rs"]
pub(crate) mod store;
#[path = "guard_tables.rs"]
mod tables;
#[cfg(test)]
#[path = "guard_tests.rs"]
mod tests;

pub(crate) use store::{ActionMeaning, GuardRule};

/// States that use up a daily total: waiting approvals reserve their amount.
const COUNTED: &str = "('pending','executing','completed','uncertain')";
pub(crate) const MEANINGS: [&str; 5] = ["read", "change", "money", "sends_outside", "delete"];
pub(super) const DAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

/// What a rule for every action of one meaning is called in its read-back.
pub(crate) fn meaning_label(meaning: &str) -> &'static str {
    match meaning {
        "read" => "Any reading action",
        "money" => "Any money action",
        "sends_outside" => "Any action that sends outside",
        "delete" => "Any delete",
        _ => "Any change",
    }
}

fn days_text(days: &[Value]) -> String {
    let names: Vec<&str> = days
        .iter()
        .filter_map(|d| d.as_u64())
        .filter(|d| (1..=7).contains(d))
        .map(|d| DAYS[d as usize - 1])
        .collect();
    match names.len() {
        0 => String::new(),
        1 => names[0].to_owned(),
        n => format!("{} and {}", names[..n - 1].join(", "), names[n - 1]),
    }
}

/// The read-back people see, made by code from the rule itself, never by a model.
pub(crate) fn summary(rule: &Value, action: &str) -> String {
    let action = rule["meaning"]
        .as_str()
        .map(meaning_label)
        .unwrap_or(action);
    let effect = if rule["effect"] == "block" {
        "block it"
    } else {
        "ask first"
    };
    if rule["kind"] == "time" {
        let (from, to) = (
            rule["value"]["from"].as_str().unwrap_or(""),
            rule["value"]["to"].as_str().unwrap_or(""),
        );
        let when = if from == to {
            "at any time".to_owned()
        } else {
            format!("between {from} and {to}")
        };
        let days = days_text(
            rule["value"]["days"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or(&[]),
        );
        return if days.is_empty() {
            format!("{action}: {when}, {effect}.")
        } else {
            format!("{action}: on {days}, {when}, {effect}.")
        };
    }
    let read = rule["source"]["title"].as_str().unwrap_or_else(|| {
        rule["source"]["operation"]
            .as_str()
            .unwrap_or("a reading action")
    });
    let matched = rule["source"]["match"]
        .as_str()
        .unwrap_or_default()
        .replace('_', " ");
    if rule["kind"] == "read_first" {
        return format!(
            "{action}: unless {read} for the same {matched} ran first in this task, {effect}."
        );
    }
    let mut field = rule["field"].as_str().unwrap_or_default().replace('_', " ");
    if !rule["source"].is_null() {
        field = format!("{field} from {read}");
    }
    let value = match &rule["value"] {
        limit if tables::is_table(limit) => tables::words(limit),
        Value::String(text) => format!("\"{text}\""),
        Value::Number(n) => n
            .as_f64()
            .map(|n| format!("{n}"))
            .unwrap_or_else(|| n.to_string()),
        other => other.to_string(),
    };
    let comparison = match rule["comparator"].as_str() {
        Some(">") => "is over",
        Some(">=") => "is at least",
        Some("<") => "is under",
        Some("<=") => "is at most",
        Some("!=") => "is not",
        _ => "is",
    };
    if rule["kind"] == "daily_total" {
        let reach = if rule["comparator"] == ">=" {
            "reach"
        } else {
            "go over"
        };
        format!("{action}: if today's total {field} would {reach} {value}, {effect}.")
    } else {
        format!("{action}: if {field} {comparison} {value}, {effect}.")
    }
}

/// `None` when the facts can't decide, for example a missing or wrongly typed field.
pub(super) fn holds(actual: &Value, comparator: &str, expected: &Value) -> Option<bool> {
    if let (Some(a), Some(b)) = (actual.as_f64(), expected.as_f64()) {
        return Some(match comparator {
            ">" => a > b,
            ">=" => a >= b,
            "<" => a < b,
            "<=" => a <= b,
            "!=" => a != b,
            _ => a == b,
        });
    }
    let same_kind = std::mem::discriminant(actual) == std::mem::discriminant(expected);
    match comparator {
        "=" if same_kind => Some(actual == expected),
        "!=" if same_kind => Some(actual != expected),
        _ => None,
    }
}

/// Whether local time `now` ("HH:MM") on ISO weekday `day` falls in a time rule's window.
pub(crate) fn in_window(window: &Value, now: &str, day: u64) -> bool {
    let days = window["days"].as_array().map(Vec::as_slice).unwrap_or(&[]);
    if !days.is_empty() && !days.iter().any(|d| d.as_u64() == Some(day)) {
        return false;
    }
    let (from, to) = (
        window["from"].as_str().unwrap_or(""),
        window["to"].as_str().unwrap_or(""),
    );
    match from.cmp(to) {
        std::cmp::Ordering::Equal => true,
        std::cmp::Ordering::Less => from <= now && now < to,
        std::cmp::Ordering::Greater => now >= from || now < to,
    }
}

/// The start of the workspace's day, its local time and ISO weekday.
async fn local_now(db: &impl ConnectionTrait, w: &str) -> Result<(i64, String, u64, i64)> {
    let row = query(db, "SELECT (extract(epoch FROM clock_timestamp())*1000)::bigint AS now_ms, floor(extract(epoch FROM date_trunc('day', clock_timestamp() AT TIME ZONE z) AT TIME ZONE z))::bigint AS start, to_char(clock_timestamp() AT TIME ZONE z,'HH24:MI') AS hm, extract(isodow FROM clock_timestamp() AT TIME ZONE z)::int AS dow FROM (SELECT COALESCE(timezone,'UTC') AS z FROM relay_workspaces WHERE id=$1) t", vec![w.into()]).await?.context("Workspace unavailable")?;
    Ok((
        row.try_get("", "start")?,
        row.try_get("", "hm")?,
        row.try_get::<i32>("", "dow")? as u64,
        row.try_get("", "now_ms")?,
    ))
}

/// The meaning of one connected action: the admin's review, or Cuckoo's suggestion.
pub(super) async fn meaning(
    db: &impl ConnectionTrait,
    w: &str,
    connection: &str,
    operation: &str,
) -> Result<String> {
    Ok(query(db, "SELECT relay_action_meaning(c.workspace,c.id,op) AS m FROM relay_control_connections c CROSS JOIN LATERAL jsonb_array_elements(c.definition->'operations') op WHERE c.workspace=$1 AND c.id=$2 AND op->>'id'=$3", vec![w.into(), connection.into(), operation.into()]).await?.context("Unsupported operation")?.try_get("", "m")?)
}

/// The latest completed read of `source.operation` on this account in this task whose
/// `source.match` field equals the request's. Agents can't supply this value.
async fn read_receipt(
    db: &impl ConnectionTrait,
    w: &str,
    job: &str,
    connection: &str,
    source: &serde_json::Map<String, Value>,
    arguments: &Value,
) -> Result<Option<(Value, i64)>> {
    let key = source
        .get("match")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let Some(wanted) = arguments.get(key).filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    let row = query(db, "SELECT result,updated FROM relay_control_actions WHERE workspace=$1 AND job=$2 AND connection=$3 AND operation=$4 AND effect='read' AND state='completed' AND arguments->$5=$6 ORDER BY updated DESC,id DESC LIMIT 1",
        vec![w.into(), job.into(), connection.into(), source.get("operation").and_then(Value::as_str).unwrap_or_default().into(), key.into(), wanted.clone().into()]).await?;
    Ok(match row {
        Some(row) => Some((
            row.try_get::<Option<Value>>("", "result")?
                .unwrap_or(Value::Null),
            row.try_get("", "updated")?,
        )),
        None => None,
    })
}

/// A dotted path in a read's result: its structured content, the result itself, or JSON
/// text in its first content item. Numbers written as text count as numbers.
pub(crate) fn fact_at(result: &Value, path: &str) -> Value {
    let text = result["content"][0]["text"]
        .as_str()
        .and_then(|t| serde_json::from_str::<Value>(t).ok());
    for root in [
        &result["structuredContent"],
        result,
        text.as_ref().unwrap_or(&Value::Null),
    ] {
        let mut value = root;
        for part in path.split('.') {
            value = &value[part];
        }
        match value {
            Value::Null => continue,
            Value::String(s) => {
                return s
                    .trim()
                    .parse::<f64>()
                    .map(|n| json!(n))
                    .unwrap_or_else(|_| value.clone());
            }
            other => return other.clone(),
        }
    }
    Value::Null
}

const RULES_FOR: &str = "SELECT to_jsonb(g) AS data FROM relay_guard_rules g WHERE g.workspace=$1 AND g.enabled AND ((g.operation=$2 AND (g.connection IS NULL OR g.connection=$3)) OR g.meaning=$4) ORDER BY g.created_ms,g.id";

/// A rule for every action of one meaning only covers actions that have its field.
fn applies(rule: &Value, schema: &Value) -> bool {
    rule["meaning"].is_null()
        || rule["kind"] == "time"
        || !rule["source"].is_null()
        || schema["properties"]
            .get(rule["field"].as_str().unwrap_or(""))
            .is_some()
}

/// The strictest company rule that applies to this exact request, as the decision
/// record saved on the action. Callers hold the workspace row lock (`workspace_tx`),
/// so two requests never both fit under the same daily total.
pub(super) async fn check(
    db: &impl ConnectionTrait,
    w: &str,
    job: &str,
    connection: &str,
    operation: &Operation,
    meaning: &str,
    arguments: &Value,
) -> Result<Option<Value>> {
    let rules = json_rows(
        db,
        RULES_FOR,
        vec![
            w.into(),
            operation.id.clone().into(),
            connection.into(),
            meaning.into(),
        ],
    )
    .await?;
    let mut strictest: Option<Value> = None;
    let mut matched = 0;
    let mut trials = vec![];
    let mut clock = None;
    for rule in rules.into_iter().filter(|r| applies(r, &operation.schema)) {
        if clock.is_none() {
            clock = Some(local_now(db, w).await?);
        }
        let (day_start, now, weekday, now_ms) = clock.clone().unwrap();
        let field = rule["field"].as_str().unwrap();
        let comparator = rule["comparator"].as_str().unwrap();
        // A value from an earlier read in this task, through Cuckoo's own receipt.
        let read = match rule["source"].as_object() {
            Some(source) => read_receipt(db, w, job, connection, source, arguments).await?,
            None => None,
        };
        let actual = match (&rule["source"], &read) {
            (Value::Null, _) => arguments.get(field).cloned().unwrap_or(Value::Null),
            (_, Some((result, _))) => fact_at(result, field),
            _ => Value::Null,
        };
        let from = read
            .as_ref()
            .map(|(_, at)| json!({"operation":rule["source"]["operation"],"read_at":at}));
        let (decides, facts) = if rule["kind"] == "read_first" {
            (
                Some(read.is_none()),
                json!({"read":rule["source"]["operation"],"match":rule["source"]["match"],"found":read.is_some()}),
            )
        } else if rule["kind"] == "time" {
            (
                Some(in_window(&rule["value"], &now, weekday)),
                json!({"local_time":now,"day":DAYS[weekday as usize - 1]}),
            )
        } else if rule["kind"] == "daily_total" {
            let (scope, target) = match rule["meaning"].as_str() {
                Some(m) => ("a.meaning=$2 AND $3::text IS NULL", m.to_owned()),
                None => (
                    "a.operation=$2 AND ($3::text IS NULL OR a.connection=$3)",
                    operation.id.clone(),
                ),
            };
            let before: f64 = query(db, &format!("SELECT COALESCE(sum((a.arguments->>$4)::numeric),0)::float8 AS total FROM relay_control_actions a WHERE a.workspace=$1 AND {scope} AND a.state IN {COUNTED} AND jsonb_typeof(a.arguments->$4)='number' AND a.created >= $5"),
                vec![w.into(), target.into(), rule["connection"].as_str().map(str::to_owned).into(), field.into(), day_start.into()]).await?.context("Workspace unavailable")?.try_get("", "total")?;
            match actual.as_f64() {
                Some(amount) => (
                    holds(&json!(before + amount), comparator, &rule["value"]),
                    json!({"field":field,"amount":amount,"today_before":before,"today_after":before + amount,"limit":rule["value"]}),
                ),
                None => (
                    None,
                    json!({"field":field,"amount":actual,"limit":rule["value"]}),
                ),
            }
        } else {
            // A limit from a Library table row, or the rule's own value.
            let (limit, from_table) = tables::limit(db, w, &rule["value"], arguments).await?;
            let mut facts = json!({"field":field,"value":actual});
            if let Some(table) = from_table {
                facts["limit"] = table;
            }
            (holds(&actual, comparator, &limit), facts)
        };
        let facts = match from {
            Some(from) => {
                let mut facts = facts;
                facts["from"] = from;
                facts
            }
            None => facts,
        };
        // Unsure means hold: facts that can't decide ask a person, even for a block rule.
        let effect = match decides {
            Some(false) => continue,
            Some(true) => rule["effect"].as_str().unwrap(),
            None => "ask",
        };
        // On trial: record what the rule would have done, and change nothing.
        if rule["trial_until_ms"]
            .as_i64()
            .is_some_and(|until| until > now_ms)
        {
            trials.push(json!({"rule": rule["id"], "effect": effect, "summary": summary(&rule, &operation.title)}));
            continue;
        }
        matched += 1;
        if strictest
            .as_ref()
            .is_none_or(|s| s["effect"] == "ask" && effect == "block")
        {
            strictest = Some(json!({
                "rule": rule["id"], "revision": rule["revision"], "effect": effect,
                "summary": summary(&rule, &operation.title), "facts": facts,
                "known": decides.is_some(),
            }));
        }
    }
    let mut record = strictest.map(|mut s| {
        s["matched"] = json!(matched);
        s
    });
    if !trials.is_empty() {
        record.get_or_insert_with(|| json!({}))["trial"] = json!(trials);
    }
    Ok(record)
}

/// The read-backs of the rules on one action, shown to the agent with its operations.
pub(super) async fn summaries(
    db: &impl ConnectionTrait,
    w: &str,
    connection: &str,
    op: &Value,
) -> Result<Vec<String>> {
    let id = op["operation"].as_str().unwrap_or_default();
    let meaning = meaning(db, w, connection, id).await?;
    let rules = json_rows(
        db,
        RULES_FOR,
        vec![w.into(), id.into(), connection.into(), meaning.into()],
    )
    .await?;
    let title = op["title"].as_str().unwrap_or(id);
    Ok(rules
        .iter()
        .filter(|r| applies(r, &op["schema"]))
        .map(|r| summary(r, title))
        .collect())
}

/// The action's mode after the Guard: block wins, then ask, then the agent's rule.
pub(super) fn stricter(mode: &str, guard: Option<&Value>) -> String {
    match (mode, guard.and_then(|g| g["effect"].as_str())) {
        ("block", _) | (_, Some("block")) => "block",
        ("ask", _) | (_, Some("ask")) => "ask",
        _ => mode,
    }
    .into()
}

#[cfg(test)]
mod pure {
    use super::*;
    #[test]
    fn facts_come_from_structured_content_or_json_text() {
        let structured = json!({"structuredContent":{"order":{"total":"2450.50"}}});
        assert_eq!(fact_at(&structured, "order.total"), json!(2450.5));
        let text = json!({"content":[{"type":"text","text":"{\"total\":90}"}]});
        assert_eq!(fact_at(&text, "total"), json!(90));
        assert_eq!(fact_at(&text, "missing"), Value::Null);
        let rule = json!({"kind":"read_first","source":{"operation":"order.read","match":"order","title":"Find an order"},"effect":"block"});
        assert_eq!(
            summary(&rule, "Update an order"),
            "Update an order: unless Find an order for the same order ran first in this task, block it."
        );
        let looked = json!({"kind":"field","field":"total","comparator":">","value":2000,"source":{"operation":"order.read","match":"order","title":"Find an order"},"effect":"ask"});
        assert_eq!(
            summary(&looked, "Refund an order"),
            "Refund an order: if total from Find an order is over 2000, ask first."
        );
    }
    #[test]
    fn windows_wrap_midnight_and_respect_days() {
        let night = json!({"from":"21:00","to":"09:00","days":[]});
        assert!(
            in_window(&night, "21:00", 3)
                && in_window(&night, "02:30", 3)
                && !in_window(&night, "09:00", 3)
                && !in_window(&night, "12:00", 3)
        );
        let weekend = json!({"from":"00:00","to":"00:00","days":[6,7]});
        assert!(in_window(&weekend, "10:00", 6) && !in_window(&weekend, "10:00", 5));
        let rule = json!({"kind":"time","meaning":"sends_outside","value":{"from":"21:00","to":"09:00","days":[6,7]},"effect":"ask"});
        assert_eq!(
            summary(&rule, "x"),
            "Any action that sends outside: on Saturday and Sunday, between 21:00 and 09:00, ask first."
        );
        let total = json!({"kind":"daily_total","meaning":"money","field":"amount","comparator":">","value":5000,"effect":"block"});
        assert_eq!(
            summary(&total, "x"),
            "Any money action: if today's total amount would go over 5000, block it."
        );
    }
}

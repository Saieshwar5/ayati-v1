//! Reviewing the tools of a connected system (C1 of the connected systems
//! plan, D249). A company's own MCP server describes its tools itself; this
//! turns each one into the action Cuckoo would review, or says plainly why it
//! can't be used yet.
//!
//! Real MCP servers describe inputs with more of JSON Schema than Cuckoo checks.
//! Details that only narrow a value (defaults, formats, lengths, ranges,
//! patterns) are left out: the server still enforces them, and Cuckoo's own
//! bounds still apply. A shape Cuckoo can't check field by field (a nested
//! object, a choice between schemas) makes the tool "Not supported" with the
//! field named, never quietly accepted.
use crate::identity::control::model::Operation;
use serde_json::{Map, Value, json};

/// Tools a system may list; more than this is refused when connecting.
pub(crate) const MAX_LISTED: usize = 200;
/// Tools one connection may have switched on.
pub(crate) const MAX_ON: usize = 20;

/// Keys that only describe or narrow a value: dropped, the server enforces them.
const NARROWING: [&str; 16] = [
    "$schema",
    "$id",
    "$comment",
    "default",
    "examples",
    "format",
    "minimum",
    "maximum",
    "exclusiveMinimum",
    "exclusiveMaximum",
    "minLength",
    "maxLength",
    "pattern",
    "multipleOf",
    "minItems",
    "uniqueItems",
];

/// One tool as the review screen shows it. `schema` is the input Cuckoo will
/// check, or `None` with `reason` when the tool can't be used yet.
#[derive(Debug, PartialEq)]
pub(crate) struct Reviewed {
    pub name: String,
    pub title: String,
    pub description: String,
    /// `read` or `write` from the tool's own hints, when it gives them.
    pub suggested: Option<&'static str>,
    pub schema: Option<Value>,
    pub reason: Option<String>,
}
impl Reviewed {
    pub(crate) fn public(&self) -> Value {
        json!({"name":self.name,"title":self.title,"description":self.description,
            "suggested":self.suggested,"supported":self.schema.is_some(),"reason":self.reason,
            "fields":self.schema.as_ref().and_then(|s| s["properties"].as_object())
                .map(|p| p.keys().cloned().collect::<Vec<_>>()).unwrap_or_default()})
    }
    /// The reviewed action, with the kind the person chose.
    pub(crate) fn operation(&self, kind: &str) -> Option<Operation> {
        Some(Operation {
            id: self.name.clone(),
            title: self.title.clone(),
            tool: self.name.clone(),
            effect: kind.to_owned(),
            schema: self.schema.clone()?,
            version: None,
        })
    }
}

/// Reviews one tool from `tools/list`.
pub(crate) fn review(tool: &Value) -> Reviewed {
    let name = tool["name"].as_str().unwrap_or_default().to_owned();
    let title = [&tool["title"], &tool["annotations"]["title"]]
        .into_iter()
        .find_map(|t| t.as_str().filter(|t| !t.trim().is_empty()))
        .unwrap_or(&name)
        .to_owned();
    let description = tool["description"].as_str().unwrap_or_default();
    let hints = &tool["annotations"];
    let suggested = if hints["readOnlyHint"] == true {
        Some("read")
    } else if hints["readOnlyHint"] == false || hints["destructiveHint"] == true {
        Some("write")
    } else {
        None
    };
    let checked = name_ok(&name).and_then(|()| schema(&tool["inputSchema"]));
    let (schema, reason) = match checked {
        Ok(schema) => (Some(schema), None),
        Err(reason) => (None, Some(reason)),
    };
    let mut reviewed = Reviewed {
        name,
        title: clip(&title, 160),
        description: clip(description, 500),
        suggested,
        schema,
        reason,
    };
    // The same checks every reviewed action passes.
    if let Some(op) = reviewed.operation("read")
        && let Err(error) = op.validate()
    {
        reviewed.schema = None;
        reviewed.reason = Some(error.to_string());
    }
    reviewed
}

fn clip(text: &str, bytes: usize) -> String {
    let text = text.trim();
    let mut end = text.len().min(bytes);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

fn name_ok(name: &str) -> Result<(), String> {
    if !name.is_empty()
        && name.len() <= 100
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b))
    {
        Ok(())
    } else {
        Err("Its name uses characters Cuckoo can't use yet.".into())
    }
}

/// The input Cuckoo checks for a tool's `inputSchema`.
pub(crate) fn schema(input: &Value) -> Result<Value, String> {
    let empty = Map::new();
    let object = match input {
        Value::Null => &empty,
        Value::Object(o) => o,
        _ => return Err("Its inputs are not described as named fields.".into()),
    };
    for key in object.keys() {
        if !NARROWING.contains(&key.as_str())
            && ![
                "type",
                "properties",
                "required",
                "additionalProperties",
                "description",
                "title",
            ]
            .contains(&key.as_str())
        {
            return Err(format!(
                "Its inputs use \u{201c}{key}\u{201d}, which Cuckoo can't check yet."
            ));
        }
    }
    if object.get("type").is_some_and(|t| t != "object") {
        return Err("Its inputs are not described as named fields.".into());
    }
    let properties = match object.get("properties") {
        None => &empty,
        Some(Value::Object(p)) => p,
        Some(_) => return Err("Its inputs are not described as named fields.".into()),
    };
    if properties.len() > 20 {
        return Err("It takes more than 20 fields.".into());
    }
    let mut fields = Map::new();
    for (name, field) in properties {
        fields.insert(name.clone(), plain(name, field)?);
    }
    let required: Vec<Value> = object
        .get("required")
        .and_then(Value::as_array)
        .map(|r| {
            r.iter()
                .filter(|v| v.as_str().is_some_and(|k| fields.contains_key(k)))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let mut out = json!({"type":"object","properties":fields,"required":required,"additionalProperties":false});
    if let Some(description) = object.get("description").and_then(Value::as_str) {
        out["description"] = json!(clip(description, 500));
    }
    Ok(out)
}

/// One field: a plain value, or a bounded list of plain values.
fn plain(name: &str, field: &Value) -> Result<Value, String> {
    let nested = || {
        format!(
            "Its field \u{201c}{name}\u{201d} is a nested object or a choice of shapes, which Cuckoo can't check yet."
        )
    };
    let object = field.as_object().ok_or_else(nested)?;
    let kind = single_type(object.get("type")).ok_or_else(nested)?;
    let mut out = Map::new();
    for (key, value) in object {
        match key.as_str() {
            "type" => {}
            "description" | "title" => {
                if let Some(text) = value.as_str() {
                    out.insert(key.clone(), json!(clip(text, 500)));
                }
            }
            "enum" => {
                out.insert(key.clone(), value.clone());
            }
            "const" => {
                out.insert("enum".into(), json!([value]));
            }
            "items" | "maxItems" if kind == "array" => {}
            key if NARROWING.contains(&key) || key == "nullable" => {}
            _ => return Err(nested()),
        }
    }
    out.insert("type".into(), json!(kind));
    if kind == "array" {
        let items = object
            .get("items")
            .and_then(Value::as_object)
            .ok_or_else(nested)?;
        let item = single_type(items.get("type"))
            .filter(|t| *t != "array")
            .ok_or_else(nested)?;
        let mut kept = json!({"type": item});
        if let Some(values) = items.get("enum") {
            kept["enum"] = values.clone();
        }
        if items.keys().any(|k| {
            !["type", "enum", "description", "title"].contains(&k.as_str())
                && !NARROWING.contains(&k.as_str())
        }) {
            return Err(nested());
        }
        // Cuckoo bounds every list at 20 items; a server allowing fewer still
        // enforces its own limit.
        let most = object
            .get("maxItems")
            .and_then(Value::as_u64)
            .map_or(20, |n| n.clamp(1, 20));
        out.insert("items".into(), kept);
        out.insert("maxItems".into(), json!(most));
    } else if !["string", "integer", "number", "boolean"].contains(&kind) {
        return Err(nested());
    }
    Ok(Value::Object(out))
}

/// `"string"`, or `["string","null"]` (an optional value), as one type.
fn single_type(value: Option<&Value>) -> Option<&str> {
    match value? {
        Value::String(t) => Some(t.as_str()),
        Value::Array(types) => {
            let real: Vec<_> = types.iter().filter(|t| *t != "null").collect();
            match real.as_slice() {
                [only] => only.as_str(),
                _ => None,
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_schema_details_are_left_to_the_server() {
        let tool = json!({"name":"get_marks","description":"A student's marks",
            "annotations":{"readOnlyHint":true},
            "inputSchema":{"$schema":"http://json-schema.org/draft-07/schema#","type":"object",
                "properties":{"student":{"type":"string","minLength":1,"maxLength":40,"pattern":"^stu-"},
                    "term":{"type":["integer","null"],"minimum":1,"maximum":3,"default":1},
                    "subjects":{"type":"array","items":{"type":"string"}},
                    "level":{"const":"school","type":"string"}},
                "required":["student","missing"]}});
        let reviewed = review(&tool);
        assert_eq!(reviewed.reason, None);
        assert_eq!(reviewed.suggested, Some("read"));
        assert_eq!(reviewed.title, "get_marks");
        let schema = reviewed.schema.unwrap();
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(schema["required"], json!(["student"]));
        assert_eq!(schema["properties"]["student"], json!({"type":"string"}));
        assert_eq!(schema["properties"]["term"], json!({"type":"integer"}));
        assert_eq!(
            schema["properties"]["subjects"],
            json!({"type":"array","items":{"type":"string"},"maxItems":20})
        );
        assert_eq!(schema["properties"]["level"]["enum"], json!(["school"]));
    }

    #[test]
    fn nested_shapes_are_not_supported_and_name_the_field() {
        let nested = review(&json!({"name":"save_plan","inputSchema":{"type":"object",
            "properties":{"plan":{"type":"object","properties":{"days":{"type":"integer"}}}}}}));
        assert!(nested.schema.is_none());
        assert!(nested.reason.unwrap().contains("\u{201c}plan\u{201d}"));
        let choice = review(&json!({"name":"find","inputSchema":{"type":"object","anyOf":[]}}));
        assert!(choice.reason.unwrap().contains("anyOf"));
        let bad_name = review(&json!({"name":"send reminder!","inputSchema":{}}));
        assert!(bad_name.schema.is_none());
    }

    #[test]
    fn a_tool_without_inputs_takes_no_fields_and_hints_suggest_its_kind() {
        let none = review(
            &json!({"name":"ping","title":"Check the service","annotations":{"destructiveHint":true}}),
        );
        assert_eq!(none.title, "Check the service");
        assert_eq!(none.suggested, Some("write"));
        assert_eq!(
            none.schema.unwrap(),
            json!({"type":"object","properties":{},"required":[],"additionalProperties":false})
        );
        assert_eq!(
            review(&json!({"name":"x","inputSchema":{}})).suggested,
            None
        );
    }

    #[test]
    fn reviewed_actions_pass_the_same_checks_as_every_other_action() {
        let reviewed = review(
            &json!({"name":"send_reminder","inputSchema":{"type":"object",
            "properties":{"student":{"type":"string"},"note":{"type":"string"}},"required":["student"]}}),
        );
        let op = reviewed.operation("write").unwrap();
        op.validate().unwrap();
        op.arguments(&json!({"student":"stu-104"})).unwrap();
        assert!(
            op.arguments(&json!({"student":"stu-104","other":1}))
                .is_err()
        );
    }
}

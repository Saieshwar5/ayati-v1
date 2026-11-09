//! Reviewed operations and exact action requests. Provider annotations never grant authority.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Operation {
    pub id: String,
    pub title: String,
    pub tool: String,
    pub effect: String,
    pub schema: Value,
    /// The provider's version of `tool` this definition was reviewed against,
    /// when the provider versions its tools (Composio does; MCP servers don't).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}
impl Operation {
    pub fn validate(&self) -> Result<()> {
        for value in [&self.id, &self.tool] {
            ensure!(
                !value.is_empty()
                    && value.len() <= 100
                    && value
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b)),
                "Invalid operation identity"
            );
        }
        ensure!(
            !self.title.trim().is_empty() && self.title.len() <= 160,
            "Invalid operation title"
        );
        ensure!(
            ["read", "write"].contains(&self.effect.as_str()),
            "Operation effects require operator review"
        );
        schema(&self.schema)
    }
    pub fn arguments(&self, input: &Value) -> Result<()> {
        self.validate()?;
        let input = input
            .as_object()
            .filter(|v| v.len() <= 20)
            .ok_or_else(|| anyhow::anyhow!("Expected bounded operation fields"))?;
        ensure!(
            serde_json::to_vec(input)?.len() <= 8000,
            "Operation input is too large"
        );
        let properties = self.schema["properties"].as_object().unwrap();
        for required in self.schema["required"].as_array().unwrap() {
            ensure!(
                input.contains_key(required.as_str().unwrap()),
                "A required operation field is missing"
            );
        }
        for (name, value) in input {
            let field = properties
                .get(name)
                .ok_or_else(|| anyhow::anyhow!("Unknown operation field"))?;
            if field["type"] == "array" {
                let items = value
                    .as_array()
                    .filter(|v| v.len() as u64 <= field["maxItems"].as_u64().unwrap())
                    .ok_or_else(|| anyhow::anyhow!("Operation list is invalid or too long"))?;
                for item in items {
                    plain_value(&field["items"], item)?;
                }
            } else {
                plain_value(field, value)?;
            }
        }
        Ok(())
    }
    /// Whether any field takes a list (only providers with list fields offer one).
    pub fn has_list_fields(&self) -> bool {
        self.schema["properties"]
            .as_object()
            .is_some_and(|p| p.values().any(|f| f["type"] == "array"))
    }
}
/// One plain value of a reviewed field: its type, size and choices.
fn plain_value(field: &Value, value: &Value) -> Result<()> {
    let valid = match field["type"].as_str().unwrap() {
        "string" => value
            .as_str()
            .is_some_and(|s| s.len() <= 2000 && !s.contains('\0')),
        "integer" => value
            .as_i64()
            .is_some_and(|n| n.unsigned_abs() <= 9_007_199_254_740_991),
        "number" => value
            .as_f64()
            .is_some_and(|n| n.is_finite() && n.abs() <= 9_007_199_254_740_991.0),
        "boolean" => value.is_boolean(),
        _ => false,
    };
    ensure!(valid, "Operation field type or size is invalid");
    if let Some(values) = field.get("enum") {
        ensure!(
            values.as_array().unwrap().contains(value),
            "Operation field is outside the supported choices"
        );
    }
    Ok(())
}
fn schema(value: &Value) -> Result<()> {
    let object = value
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("Unsupported operation schema"))?;
    ensure!(
        object.keys().all(|k| [
            "type",
            "properties",
            "required",
            "additionalProperties",
            "description",
            "title"
        ]
        .contains(&k.as_str())),
        "This operation schema needs additional support"
    );
    ensure!(
        value["type"] == "object" && value["additionalProperties"] == false,
        "Operations require a closed object schema"
    );
    let fields = value["properties"]
        .as_object()
        .filter(|p| p.len() <= 20)
        .ok_or_else(|| anyhow::anyhow!("Unsupported operation fields"))?;
    let required = value["required"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("Operation required fields must be explicit"))?;
    ensure!(
        required
            .iter()
            .all(|v| v.as_str().is_some_and(|k| fields.contains_key(k))),
        "Invalid required fields"
    );
    for (name, field) in fields {
        ensure!(
            !name.is_empty()
                && name.len() <= 80
                && field.as_object().is_some_and(|v| v.keys().all(|k| [
                    "type",
                    "description",
                    "title",
                    "enum",
                    "items",
                    "maxItems"
                ]
                .contains(&k.as_str()))),
            "Unsupported operation field schema"
        );
        if field["type"] == "array" {
            // A bounded list of one plain type, such as cc or attendees.
            ensure!(
                field.get("enum").is_none()
                    && field["maxItems"]
                        .as_u64()
                        .is_some_and(|n| (1..=20).contains(&n))
                    && field["items"]
                        .as_object()
                        .is_some_and(|v| v.keys().all(|k| ["type", "enum"].contains(&k.as_str()))),
                "Operation lists need one plain item type and at most 20 items"
            );
            plain_field(&field["items"])?;
            continue;
        }
        ensure!(
            field.get("items").is_none() && field.get("maxItems").is_none(),
            "Unsupported operation field schema"
        );
        plain_field(field)?;
    }
    Ok(())
}
/// A field of one plain type, with valid choices if it lists them.
fn plain_field(field: &Value) -> Result<()> {
    ensure!(
        field["type"]
            .as_str()
            .is_some_and(|s| ["string", "integer", "number", "boolean"].contains(&s)),
        "Nested or dynamic operation schemas are not supported yet"
    );
    if let Some(values) = field.get("enum") {
        ensure!(
            values.as_array().is_some_and(|a| !a.is_empty()
                && a.len() <= 30
                && a.iter().all(|v| match field["type"].as_str() {
                    Some("string") => v.is_string(),
                    Some("integer") => v.as_i64().is_some(),
                    Some("number") => v.is_number(),
                    Some("boolean") => v.is_boolean(),
                    _ => false,
                })),
            "Unsupported operation choices"
        );
    }
    Ok(())
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
/// An agent's own rule for one action; it can only be stricter than the admin's.
pub(crate) struct Rule {
    pub connection: String,
    pub operation: String,
    pub mode: String,
    pub revision: i64,
}
/// Switches a connector on or off for one agent.
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AgentConnection {
    pub connection: String,
    pub enabled: bool,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ActionRequest {
    pub connection: String,
    pub operation: String,
    pub arguments: Value,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Decision {
    pub id: String,
    pub decision: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn reviewed_schemas_reject_unknown_constraints_and_unbounded_inputs() {
        let mut op = Operation {
            id: "order.read".into(),
            title: "Read order".into(),
            tool: "read_order".into(),
            effect: "read".into(),
            schema: json!({"type":"object","properties":{"id":{"type":"string"}},"required":["id"],"additionalProperties":false}),
            version: None,
        };
        assert!(op.arguments(&json!({"id":"one"})).is_ok());
        for input in [
            json!({}),
            json!({"id":42}),
            json!({"id":"one","admin":true}),
        ] {
            assert!(op.arguments(&input).is_err());
        }
        op.schema["properties"]["id"]["pattern"] = json!("[a-z]+");
        assert!(op.validate().is_err());
    }

    #[test]
    fn list_fields_take_bounded_lists_of_one_plain_type() {
        let mut op = Operation {
            id: "gmail.send_email".into(),
            title: "Send an email".into(),
            tool: "GMAIL_SEND_EMAIL".into(),
            effect: "write".into(),
            schema: json!({"type":"object","properties":{
                "to":{"type":"string"},
                "cc":{"type":"array","items":{"type":"string"},"maxItems":3,"description":"Copies"},
                "days":{"type":"array","items":{"type":"integer","enum":[1,2]},"maxItems":2}},
                "required":["to"],"additionalProperties":false}),
            version: None,
        };
        assert!(op.has_list_fields());
        op.arguments(&json!({"to":"a@x.test","cc":["b@x.test","c@x.test"],"days":[2]}))
            .unwrap();
        op.arguments(&json!({"to":"a@x.test","cc":[]})).unwrap();
        for input in [
            json!({"to":"a@x.test","cc":"b@x.test"}),
            json!({"to":"a@x.test","cc":["a","b","c","d"]}),
            json!({"to":"a@x.test","cc":[1]}),
            json!({"to":"a@x.test","cc":[["nested"]]}),
            json!({"to":"a@x.test","days":[3]}),
            json!({"to":["a@x.test"]}),
        ] {
            assert!(op.arguments(&input).is_err(), "{input}");
        }
        // Lists must be bounded, of one plain type, and nothing else.
        for field in [
            json!({"type":"array","items":{"type":"string"}}),
            json!({"type":"array","items":{"type":"string"},"maxItems":21}),
            json!({"type":"array","items":{"type":"object"},"maxItems":2}),
            json!({"type":"array","items":{"type":"string","pattern":"x"},"maxItems":2}),
            json!({"type":"string","maxItems":2}),
        ] {
            op.schema["properties"]["cc"] = field.clone();
            assert!(op.validate().is_err(), "{field}");
        }
    }
}

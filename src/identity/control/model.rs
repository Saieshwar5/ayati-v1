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
        }
        Ok(())
    }
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
                    "enum"
                ]
                .contains(&k.as_str()))),
            "Unsupported operation field schema"
        );
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
    }
    Ok(())
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Rule {
    #[serde(default)]
    pub approver: Option<String>,
    #[serde(default)]
    pub approver_revision: Option<i64>,
    #[serde(default)]
    pub approval_chat: Option<String>,
    pub connection: String,
    pub operation: String,
    pub mode: String,
    pub revision: i64,
    pub chat: String,
    #[serde(default)]
    pub delegation_revision: Option<i64>,
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
}

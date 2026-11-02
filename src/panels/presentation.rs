//! Separate changing observations from the reviewed meaning and presentation.
use serde_json::Value;

pub fn shape(mut definition: Value) -> Value {
    match definition["template"].as_str() {
        Some("list" | "agenda") => definition["template"] = Value::String("table".into()),
        Some("brief") => definition["template"] = Value::String("text".into()),
        _ => {}
    }
    if let Some(sections) = definition["sections"].as_array_mut() {
        for section in sections {
            let Some(fields) = section.as_object_mut() else {
                continue;
            };
            if fields.get("kind").and_then(Value::as_str) == Some("chart") {
                fields
                    .entry("variant")
                    .or_insert_with(|| Value::String("line".into()));
            }
            for field in ["body", "value", "rows", "points", "events"] {
                fields.remove(field);
            }
            if let Some(comparison) = fields.get_mut("comparison").and_then(Value::as_object_mut) {
                comparison.remove("value");
            }
            if let Some(series) = fields.get_mut("series").and_then(Value::as_array_mut) {
                for value in series {
                    if let Some(value) = value.as_object_mut() {
                        value.remove("points");
                    }
                }
            }
            match fields.get("kind").and_then(Value::as_str) {
                Some("collection") => {
                    fields.remove("items");
                }
                // A pipeline's stages are its shape; how many and how much change.
                Some("pipeline") => {
                    if let Some(stages) = fields.get_mut("stages").and_then(Value::as_array_mut) {
                        for stage in stages {
                            if let Some(stage) = stage.as_object_mut() {
                                stage.remove("count");
                                stage.remove("value");
                            }
                        }
                    }
                }
                Some("metrics" | "progress") => {
                    if let Some(items) = fields.get_mut("items").and_then(Value::as_array_mut) {
                        for item in items {
                            if let Some(item) = item.as_object_mut() {
                                item.remove("actual");
                                item.remove("value");
                                if let Some(c) =
                                    item.get_mut("comparison").and_then(Value::as_object_mut)
                                {
                                    c.remove("value");
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    definition
}

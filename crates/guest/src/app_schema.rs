//! Tools for an agent asked to build an app (Apps BA3); the controller decides access.
use serde_json::{Value, json};

pub(crate) fn tools() -> Vec<Value> {
    let app = json!({"type":"string","maxLength":64});
    let path = json!({"type":"string","maxLength":200});
    vec![
        json!({"name":"my_apps","description":"The apps the person you are working for asked you to build: id, name, description, draft revision and published version.","inputSchema":{"type":"object","properties":{},"required":[],"additionalProperties":false}}),
        json!({"name":"app_files","description":"One app's draft: its files with sizes, the draft revision and what the release checks find.","inputSchema":{"type":"object","properties":{"app":app},"required":["app"],"additionalProperties":false}}),
        json!({"name":"read_app_file","description":"Read one file of an app's draft.","inputSchema":{"type":"object","properties":{"app":app,"path":path},"required":["app","path"],"additionalProperties":false}}),
        json!({"name":"write_app_file","description":"Write one whole file of an app's draft, from the draft revision you read; returns the new revision and what the checks find. You can't publish; the person does.","inputSchema":{"type":"object","properties":{"app":app,"path":path,"content":{"type":"string","maxLength":262144},"revision":{"type":"integer","minimum":1}},"required":["app","path","content","revision"],"additionalProperties":false}}),
    ]
}

pub(crate) fn observed(name: &str, input: &Value) -> bool {
    let Some(v) = input.as_object() else {
        return false;
    };
    let text = |key: &str, max: usize| {
        v.get(key)
            .and_then(Value::as_str)
            .is_some_and(|s| s.len() <= max)
    };
    let only = |keys: &[&str]| v.keys().all(|k| keys.contains(&k.as_str()));
    match name {
        "my_apps" => v.is_empty(),
        "app_files" => only(&["app"]) && text("app", 64),
        "read_app_file" => only(&["app", "path"]) && text("app", 64) && text("path", 200),
        "write_app_file" => {
            only(&["app", "path", "content", "revision"])
                && text("app", 64)
                && text("path", 200)
                && text("content", 262_144)
                && v.get("revision").is_some_and(Value::is_u64)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    #[test]
    fn inputs_are_bounded() {
        assert!(super::observed("my_apps", &json!({})));
        assert!(!super::observed("my_apps", &json!({"app":"a"})));
        assert!(super::observed(
            "write_app_file",
            &json!({"app":"a","path":"app.js","content":"x","revision":2})
        ));
        assert!(!super::observed(
            "write_app_file",
            &json!({"app":"a","path":"app.js","content":"x","revision":-1})
        ));
        assert!(!super::observed(
            "read_app_file",
            &json!({"app":"a","path":"x","extra":1})
        ));
        assert_eq!(super::tools().len(), 4);
    }
}

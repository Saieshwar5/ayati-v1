//! Pipedream component props, read as the JSON Schema that `curated::project`
//! reduces to Cuckoo's reviewed fields. Components describe their inputs as
//! `configurable_props`, not JSON Schema; this keeps only plain text, number and
//! true-or-false props. Everything else is marked unsupported, so `project`
//! leaves it out, or refuses the action when it is required.
use anyhow::{Context, Result, ensure};
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;

/// One component version, as Cuckoo can offer it.
pub(super) struct Component {
    /// The exact version read, which calls and deployments pin.
    pub version: String,
    /// The prop that takes the connected account (`{authProvisionId}`).
    pub app_prop: String,
    /// `{type, properties, required}` for `curated::project`.
    pub schema: Value,
    /// `string[]` props offered as one plain value and sent as a one-item list.
    pub lists: BTreeSet<String>,
    /// The component's own `readOnlyHint`, when it gives one.
    pub read_only: Option<bool>,
}

/// Props Pipedream fills in itself, or that only describe the component.
const MANAGED: [&str; 6] = [
    "$.service.db",
    "$.interface.http",
    "$.interface.timer",
    "$.interface.apphook",
    "alert",
    "dir",
];

/// Reads a component answer's `data` for `key`, at `version` when one is
/// pinned. The component must take exactly one account, of `app`, and must
/// not need File Stash.
pub(super) fn read(data: &Value, key: &str, version: Option<&str>, app: &str) -> Result<Component> {
    ensure!(
        data["key"] == key,
        "Pipedream returned a different component"
    );
    let current = data["version"]
        .as_str()
        .filter(|v| semantic_version(v))
        .context("Pipedream omitted the component version")?;
    ensure!(
        version.is_none_or(|v| v == current),
        "Pipedream returned a different component version"
    );
    ensure!(
        data["stash"] != "required",
        "The component needs File Stash"
    );
    let mut app_prop = None;
    let mut properties = Map::new();
    let mut required = vec![];
    let mut lists = BTreeSet::new();
    for prop in data["configurable_props"]
        .as_array()
        .context("Pipedream omitted the component's props")?
    {
        let name = prop["name"]
            .as_str()
            .context("A component prop has no name")?;
        let kind = prop["type"].as_str().unwrap_or_default();
        if prop["hidden"] == true || prop["disabled"] == true || prop["readOnly"] == true {
            continue;
        }
        if kind == "app" {
            ensure!(
                prop["app"] == app && app_prop.is_none(),
                "The component takes another app's account"
            );
            app_prop = Some(name.to_owned());
            continue;
        }
        if MANAGED.contains(&kind) {
            continue;
        }
        let (field, list) = field(prop, kind);
        if list {
            lists.insert(name.to_owned());
        }
        properties.insert(name.to_owned(), field);
        let defaulted = prop.get("default").is_some_and(|d| !d.is_null());
        if prop["optional"] != true && !defaulted {
            required.push(name);
        }
    }
    Ok(Component {
        version: current.to_owned(),
        app_prop: app_prop.context("The component takes no account for this app")?,
        schema: json!({"type":"object","properties":properties,"required":required}),
        lists,
        read_only: data["annotations"]["readOnlyHint"].as_bool(),
    })
}

/// One prop as a JSON Schema field, and whether it is a list sent as one item.
/// Secret, labelled, dynamic (`reloadProps`) and formatted (file) props are
/// unsupported, as is every type other than string, integer, boolean and
/// `string[]`.
fn field(prop: &Value, kind: &str) -> (Value, bool) {
    let unsupported = (json!({"type": "unsupported"}), false);
    let (plain, list) = match kind {
        "string" | "integer" | "boolean" => (kind, false),
        "string[]" => ("string", true),
        _ => return unsupported,
    };
    if prop["secret"] == true
        || prop["withLabel"] == true
        || prop["reloadProps"] == true
        || prop.get("format").is_some_and(|f| !f.is_null())
    {
        return unsupported;
    }
    let mut field = json!({"type": plain});
    let text = prop["description"].as_str().or(prop["label"].as_str());
    match (text, list) {
        (Some(text), true) => field["description"] = json!(format!("One value only. {text}")),
        (None, true) => field["description"] = json!("One value only."),
        (Some(text), false) => field["description"] = json!(text),
        (None, false) => {}
    }
    if let Some(options) = prop.get("options").filter(|o| !o.is_null()) {
        // Static choices: plain values, or {label, value} pairs. `plain` in
        // curated.rs checks their type and count.
        let values: Option<Vec<Value>> = options.as_array().and_then(|options| {
            options
                .iter()
                .map(|o| match o {
                    Value::Object(pair) => pair.get("value").cloned(),
                    value => Some(value.clone()),
                })
                .collect()
        });
        match values {
            Some(values) => field["enum"] = json!(values),
            None => return unsupported,
        }
    }
    (field, list)
}

fn semantic_version(value: &str) -> bool {
    let parts: Vec<&str> = value.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.len() <= 9 && p.bytes().all(|b| b.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::connectors::curated;

    fn send_email(extra: Value) -> Value {
        let mut props = vec![
            json!({"name":"gmail","type":"app","app":"gmail"}),
            json!({"name":"to","type":"string[]","description":"Recipients","optional":true}),
            json!({"name":"subject","type":"string","optional":true}),
            json!({"name":"body","type":"string"}),
            json!({"name":"bodyType","type":"string","optional":true,"default":"plaintext",
                "options":[{"label":"Plain text","value":"plaintext"},{"label":"HTML","value":"html"}]}),
            json!({"name":"attachments","type":"string[]","format":"file-ref","optional":true}),
            json!({"name":"syncDir","type":"dir","sync":true,"optional":true}),
        ];
        props.push(extra);
        json!({"key":"gmail-send-email","version":"0.3.4","configurable_props":props,
            "annotations":{"readOnlyHint":false}})
    }

    fn project(data: &Value) -> Option<Value> {
        let send = curated::app("gmail")
            .unwrap()
            .actions
            .iter()
            .find(|a| a.id == "gmail.send_email")
            .unwrap();
        let component = read(data, "gmail-send-email", Some("0.3.4"), "gmail").ok()?;
        curated::project(send, &send.providers["pipedream"], &component.schema)
    }

    #[test]
    fn component_props_are_reduced_to_reviewed_plain_fields() {
        let data = send_email(json!({"name":"replyAll","type":"boolean","optional":true}));
        let component = read(&data, "gmail-send-email", Some("0.3.4"), "gmail").unwrap();
        assert_eq!(component.app_prop, "gmail");
        assert_eq!(component.read_only, Some(false));
        assert_eq!(component.lists, BTreeSet::from(["to".to_owned()]));
        assert_eq!(component.schema["required"], json!(["body"]));
        assert_eq!(
            component.schema["properties"]["bodyType"]["enum"],
            json!(["plaintext", "html"])
        );
        assert_eq!(
            component.schema["properties"]["attachments"]["type"],
            "unsupported"
        );
        let schema = project(&data).unwrap();
        assert_eq!(schema["required"], json!(["body", "subject", "to"]));
        assert_eq!(
            schema["properties"]["to"],
            json!({"type":"string","description":"One value only. Recipients"})
        );
        // Another version, another key or another app's account is not this component.
        assert!(read(&data, "gmail-send-email", Some("0.3.5"), "gmail").is_err());
        assert!(read(&data, "gmail-create-draft", None, "gmail").is_err());
        assert!(read(&data, "gmail-send-email", None, "slack").is_err());
    }

    #[test]
    fn actions_whose_required_props_cuckoo_does_not_offer_are_refused() {
        // A required prop no Cuckoo field maps to.
        assert!(project(&send_email(json!({"name":"fromEmail","type":"string"}))).is_none());
        // Required props Cuckoo can't send as plain values.
        for unsupported in [
            json!({"name":"fromEmail","type":"string","withLabel":true}),
            json!({"name":"labels","type":"integer[]"}),
            json!({"name":"auth","type":"string","secret":true}),
            json!({"name":"other","type":"app","app":"slack"}),
        ] {
            assert!(
                project(&send_email(unsupported.clone())).is_none(),
                "{unsupported}"
            );
        }
        // A dynamic prop Cuckoo maps changes the other props, so it is not offered.
        let mut dynamic = send_email(json!({"name":"replyAll","type":"boolean","optional":true}));
        dynamic["configurable_props"][3]["reloadProps"] = json!(true);
        assert!(project(&dynamic).is_none());
        // A component that needs File Stash is not offered at all.
        let mut stash = send_email(json!({"name":"replyAll","type":"boolean","optional":true}));
        stash["stash"] = json!("required");
        assert!(project(&stash).is_none());
    }
}

//! Cuckoo's own catalog of connector apps and actions: the standard names.
//! Rules, approvals, schedules and agent instructions refer to these ids, never
//! to a provider's tool names, so an app can move to another provider, or to a
//! connector Cuckoo builds itself, without changing what people approved.
//! Each provider maps an action to its own tool and field names here.
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Catalog {
    pub apps: Vec<CatalogApp>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CatalogApp {
    pub id: String,
    pub name: String,
    /// One of [`CATEGORIES`]: how the Add app catalog groups apps.
    pub category: String,
    /// A few words on what the app is, shown under its name.
    pub description: String,
    /// Provider name to the provider's own app identity (Composio's toolkit).
    pub providers: BTreeMap<String, String>,
    pub actions: Vec<Action>,
    /// Events agents can be started by, under Cuckoo's names.
    #[serde(default)]
    pub triggers: Vec<Trigger>,
    /// Per provider, the read that proves which account signed in.
    #[serde(default)]
    pub identity: BTreeMap<String, IdentityRead>,
}
/// A read that proves which account signed in: a provider tool run with fixed
/// `arguments` only, and the field of its answer that holds the account's stable
/// identity, such as the mailbox address. Reconnecting the same identity keeps
/// the account's rules.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IdentityRead {
    pub tool: String,
    #[serde(default)]
    pub arguments: Map<String, Value>,
    /// A dotted path, such as `user.emailAddress`.
    pub field: String,
}
/// The categories of the Add app catalog, in screen order: (id, name).
pub(crate) const CATEGORIES: [(&str, &str); 8] = [
    ("email_calendar", "Email and calendar"),
    ("files", "Files and documents"),
    ("sales", "Sales and customers"),
    ("accounting", "Accounting and payments"),
    ("messaging", "Messaging"),
    ("projects", "Projects and tasks"),
    ("support", "Support"),
    ("shops", "Shops"),
];
/// An app event, such as "New email". Like an action, each provider maps it to
/// its own trigger and field names; only the mapped scalar fields (at most
/// eight) ever reach an agent.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Trigger {
    pub id: String,
    pub title: String,
    pub providers: BTreeMap<String, Binding>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Action {
    pub id: String,
    pub title: String,
    pub effect: String,
    /// Fields Cuckoo requires even where the provider leaves them optional.
    #[serde(default)]
    pub required: Vec<String>,
    pub providers: BTreeMap<String, Binding>,
}
/// How one provider runs an action: its tool, and Cuckoo field names mapped to
/// the provider's. Only these fields are ever offered to an agent.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Binding {
    pub tool: String,
    pub fields: BTreeMap<String, String>,
}

static CATALOG: std::sync::OnceLock<Catalog> = std::sync::OnceLock::new();

/// The catalog shipped with this release, checked once.
pub(crate) fn catalog() -> &'static Catalog {
    CATALOG.get_or_init(|| {
        let catalog: Catalog =
            serde_json::from_str(include_str!("catalog.json")).expect("valid connector catalog");
        validate(&catalog).expect("valid connector catalog");
        catalog
    })
}

pub(crate) fn app(id: &str) -> Option<&'static CatalogApp> {
    catalog().apps.iter().find(|a| a.id == id)
}

fn validate(catalog: &Catalog) -> Result<()> {
    let mut apps = std::collections::HashSet::new();
    let mut actions = std::collections::HashSet::new();
    for app in &catalog.apps {
        crate::identity::identifier(&app.id)?;
        ensure!(apps.insert(&app.id), "Duplicate connector app {}", app.id);
        ensure!(!app.name.trim().is_empty() && !app.actions.is_empty() && app.actions.len() <= 20);
        ensure!(
            CATEGORIES.iter().any(|(id, _)| *id == app.category)
                && !app.description.trim().is_empty()
                && app.description.len() <= 80,
            "{} needs a known category and a short description",
            app.id
        );
        ensure!(app.triggers.len() <= 20, "Too many triggers for {}", app.id);
        for (provider, read) in &app.identity {
            ensure!(
                app.providers.contains_key(provider)
                    && !read.tool.is_empty()
                    && read.arguments.len() <= 5
                    && read
                        .field
                        .split('.')
                        .all(|part| crate::identity::identifier(part).is_ok()),
                "{} has an invalid {provider} identity read",
                app.id
            );
        }
        for trigger in &app.triggers {
            ensure!(
                trigger.id.starts_with(&format!("{}.", app.id)) && actions.insert(&trigger.id),
                "Trigger {} must be unique and named after its app",
                trigger.id
            );
            for (provider, binding) in &trigger.providers {
                ensure!(
                    app.providers.contains_key(provider),
                    "{} has no {provider} app",
                    app.id
                );
                ensure!(
                    !binding.fields.is_empty() && binding.fields.len() <= 8,
                    "{} must map one to eight event fields",
                    trigger.id
                );
            }
        }
        for action in &app.actions {
            ensure!(
                action.id.starts_with(&format!("{}.", app.id)) && actions.insert(&action.id),
                "Action {} must be unique and named after its app",
                action.id
            );
            ensure!(["read", "write"].contains(&action.effect.as_str()));
            for (provider, binding) in &action.providers {
                ensure!(
                    app.providers.contains_key(provider),
                    "{} has no {provider} app",
                    app.id
                );
                ensure!(!binding.fields.is_empty() && binding.fields.len() <= 20);
                ensure!(
                    action
                        .required
                        .iter()
                        .all(|f| binding.fields.contains_key(f)),
                    "{} requires a field its {provider} binding does not map",
                    action.id
                );
            }
        }
    }
    Ok(())
}

/// The closed schema an agent sees for an action, built from the provider's own
/// input schema: only the reviewed fields, under Cuckoo's names, plain values
/// only. `None` when the provider requires a field Cuckoo doesn't offer, or a
/// field Cuckoo requires isn't a plain value at the provider.
pub(crate) fn project(action: &Action, binding: &Binding, provider: &Value) -> Option<Value> {
    let properties = provider["properties"].as_object()?;
    let provider_required: Vec<&str> = provider["required"]
        .as_array()
        .map(|r| r.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let mut required: Vec<String> = action.required.clone();
    for name in provider_required {
        let (cuckoo, _) = binding.fields.iter().find(|(_, p)| p.as_str() == name)?;
        required.push(cuckoo.clone());
    }
    required.sort();
    required.dedup();
    let mut fields = Map::new();
    for (cuckoo, name) in &binding.fields {
        match properties
            .get(name)
            .and_then(|f| plain(f).or_else(|| listed(f)))
        {
            Some(field) => {
                fields.insert(cuckoo.clone(), field);
            }
            None if required.contains(cuckoo) => return None,
            None => {}
        }
    }
    Some(
        json!({"type":"object","properties":fields,"required":required,"additionalProperties":false}),
    )
}

/// A provider field as Cuckoo offers it: one plain type, a bounded description
/// and, if the provider lists them, valid choices. Anything else is left out.
fn plain(field: &Value) -> Option<Value> {
    let kind = field["type"].as_str()?;
    if !["string", "integer", "number", "boolean"].contains(&kind) {
        return None;
    }
    let mut out = json!({"type": kind});
    if let Some(text) = field["description"]
        .as_str()
        .filter(|t| !t.trim().is_empty())
    {
        out["description"] = json!(text.chars().take(500).collect::<String>());
    }
    if let Some(choices) = field.get("enum") {
        let valid = choices.as_array().is_some_and(|c| {
            !c.is_empty()
                && c.len() <= 30
                && c.iter().all(|v| match kind {
                    "string" => v.is_string(),
                    "integer" => v.as_i64().is_some(),
                    "number" => v.is_number(),
                    _ => v.is_boolean(),
                })
        });
        if !valid {
            return None;
        }
        out["enum"] = choices.clone();
    }
    Some(out)
}

/// A provider field that takes a list of plain values (cc, attendees), as a
/// bounded list of one plain type. Items the provider also accepts as objects
/// are offered as their plain alternative only. Anything else is left out.
fn listed(field: &Value) -> Option<Value> {
    if field["type"] != "array" {
        return None;
    }
    let items = &field["items"];
    let item = plain(items).or_else(|| {
        ["anyOf", "oneOf"]
            .iter()
            .filter_map(|k| items[*k].as_array())
            .flatten()
            .find_map(plain)
    })?;
    let mut item = item.as_object()?.clone();
    item.remove("description");
    let most = field["maxItems"]
        .as_u64()
        .filter(|n| (1..=MAX_ITEMS).contains(n))
        .unwrap_or(MAX_ITEMS);
    let mut out = json!({"type":"array","items":item,"maxItems":most});
    if let Some(text) = field["description"]
        .as_str()
        .filter(|t| !t.trim().is_empty())
    {
        out["description"] = json!(text.chars().take(500).collect::<String>());
    }
    Some(out)
}
/// The most values a list field takes.
pub(crate) const MAX_ITEMS: u64 = 20;

/// The account identity an identity read's answer holds: the field at the top of
/// the answer, or inside the `data` or `response_data` some tools wrap it in.
pub(crate) fn identity_value(answer: &Value, field: &str) -> Option<String> {
    let at = |root: &Value| {
        field
            .split('.')
            .try_fold(root, |value, part| value.get(part))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|v| {
                !v.is_empty() && v.chars().count() <= 320 && !v.chars().any(char::is_control)
            })
            .map(str::to_owned)
    };
    at(answer)
        .or_else(|| at(&answer["data"]))
        .or_else(|| at(&answer["response_data"]))
}

/// Cuckoo's argument names translated to the provider's.
pub(crate) fn provider_arguments(
    fields: &BTreeMap<String, String>,
    arguments: &Value,
) -> Result<Value> {
    let mut out = Map::new();
    for (name, value) in arguments.as_object().context("Expected operation fields")? {
        let field = fields.get(name).context("Unknown operation field")?;
        out.insert(field.clone(), value.clone());
    }
    Ok(Value::Object(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_catalog_is_valid_and_named_by_app() {
        let catalog = catalog();
        assert!(catalog.apps.iter().any(|a| a.id == "gmail"));
        for app in &catalog.apps {
            assert!(app.actions.iter().all(|a| a.id.starts_with(&app.id)));
        }
    }

    #[test]
    fn provider_schemas_are_reduced_to_reviewed_plain_fields_under_cuckoo_names() {
        let send = app("gmail")
            .unwrap()
            .actions
            .iter()
            .find(|a| a.id == "gmail.send_email")
            .unwrap();
        let binding = &send.providers["composio"];
        let provider = json!({"type":"object","properties":{
            "recipient_email":{"type":"string","description":"Who gets it"},
            "subject":{"type":"string"},"body":{"type":"string"},"is_html":{"type":"boolean"},
            "cc":{"type":"array","items":{"type":"string"}},"user_id":{"type":"string","default":"me"}},
            "required":[]});
        let schema = project(send, binding, &provider).unwrap();
        assert_eq!(schema["required"], json!(["body", "subject", "to"]));
        assert_eq!(
            schema["properties"]["to"],
            json!({"type":"string","description":"Who gets it"})
        );
        assert_eq!(
            schema["properties"]["cc"],
            json!({"type":"array","items":{"type":"string"},"maxItems":20})
        );
        assert!(schema["properties"].get("user_id").is_none());
        let operation = crate::identity::control::model::Operation {
            id: send.id.clone(),
            title: send.title.clone(),
            tool: binding.tool.clone(),
            effect: send.effect.clone(),
            schema,
            version: Some("20260901_00".into()),
        };
        operation.validate().unwrap();
        let args = json!({"to":"a@example.test","subject":"Hi","body":"Hello"});
        operation.arguments(&args).unwrap();
        assert_eq!(
            provider_arguments(&binding.fields, &args).unwrap()["recipient_email"],
            "a@example.test"
        );
        // A provider that requires a field Cuckoo doesn't offer makes the action unavailable.
        let mut strict = provider.clone();
        strict["required"] = json!(["user_id"]);
        assert!(project(send, binding, &strict).is_none());
        // So does a required field that isn't a plain value at the provider.
        let mut nested = provider;
        nested["properties"]["recipient_email"] = json!({"type":"array"});
        assert!(project(send, binding, &nested).is_none());
    }

    #[test]
    fn list_fields_are_bounded_lists_of_plain_values_and_anything_else_is_left_out() {
        let attendees = |items: Value| json!({"type":"array","items":items,"description":"Guests"});
        let string =
            json!({"type":"array","items":{"type":"string"},"maxItems":20,"description":"Guests"});
        // Items given as an email or an object are offered as emails.
        let either = attendees(json!({"anyOf":[{"type":"string"},{"type":"object"}]}));
        assert_eq!(listed(&either), Some(string.clone()));
        assert_eq!(
            listed(&attendees(json!({"type":"string","description":"x"}))),
            Some(string)
        );
        let mut few = attendees(json!({"type":"integer","enum":[1,2]}));
        few["maxItems"] = json!(5);
        assert_eq!(
            listed(&few).unwrap(),
            json!({"type":"array","items":{"type":"integer","enum":[1,2]},"maxItems":5,"description":"Guests"})
        );
        for field in [
            attendees(json!({"type":"object"})),
            attendees(json!({"type":"array","items":{"type":"string"}})),
            json!({"type":"array"}),
            json!({"type":"object"}),
        ] {
            assert!(listed(&field).is_none(), "{field}");
        }
        // The calendar's attendees reach the agent as a list it can fill.
        let create = app("google_calendar")
            .unwrap()
            .actions
            .iter()
            .find(|a| a.id == "google_calendar.create_event")
            .unwrap();
        let provider = json!({"type":"object","properties":{"summary":{"type":"string"},
            "start_datetime":{"type":"string"},"attendees":either},"required":[]});
        let schema = project(create, &create.providers["composio"], &provider).unwrap();
        assert_eq!(schema["properties"]["attendees"]["maxItems"], 20);
        let operation = crate::identity::control::model::Operation {
            id: create.id.clone(),
            title: create.title.clone(),
            tool: "GOOGLECALENDAR_CREATE_EVENT".into(),
            effect: "write".into(),
            schema,
            version: None,
        };
        operation
            .arguments(
                &json!({"title":"Review","start":"2026-11-09T10:00:00","attendees":["a@x.test"]}),
            )
            .unwrap();
    }

    #[test]
    fn identity_reads_keep_one_plain_field_wherever_the_tool_wraps_it() {
        let answer = json!({"emailAddress":" orders@shop.test ","messagesTotal":4});
        assert_eq!(
            identity_value(&answer, "emailAddress").as_deref(),
            Some("orders@shop.test")
        );
        let wrapped = json!({"response_data":{"user":{"emailAddress":"a@x.test"}}});
        assert_eq!(
            identity_value(&wrapped, "user.emailAddress").as_deref(),
            Some("a@x.test")
        );
        assert_eq!(
            identity_value(&json!({"data":{"id":"b@x.test"}}), "id").as_deref(),
            Some("b@x.test")
        );
        for answer in [
            json!({}),
            json!({"emailAddress":""}),
            json!({"emailAddress":7}),
            json!({"emailAddress":"a\nb"}),
        ] {
            assert!(
                identity_value(&answer, "emailAddress").is_none(),
                "{answer}"
            );
        }
        assert!(app("gmail").unwrap().identity.contains_key("composio"));
    }
}

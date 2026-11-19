//! Apps a workspace's admins added from a provider's library (schema 135).
//!
//! An added app is a catalog app that belongs to one workspace: the admin chose
//! its actions and said which only read. It is offered, allowed and connected
//! like a built-in app. Its actions keep the provider's own field names, so a
//! saved connection carries everything a call needs: the reviewed fields are the
//! fields of its saved schema.
use super::curated::{self, Action, Binding, CatalogApp};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// The most actions one added app has, as for built-in apps.
pub(crate) const MAX_ACTIONS: usize = 20;
/// The most fields one action offers.
pub(crate) const MAX_FIELDS: usize = 20;
/// The most apps one workspace adds.
pub(crate) const MAX_APPS: usize = 30;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AddedAction {
    pub id: String,
    pub title: String,
    pub effect: String,
    /// The provider's tool.
    pub tool: String,
    /// The provider's fields the admin's review covers, under their own names.
    pub fields: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AddedApp {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub provider: String,
    /// The provider's own name for the app (Composio's toolkit).
    pub toolkit: String,
    pub actions: Vec<AddedAction>,
}

impl AddedApp {
    pub(crate) fn validate(&self) -> Result<()> {
        crate::identity::identifier(&self.id)?;
        ensure!(
            curated::app(&self.id).is_none(),
            "{} is already one of Cuckoo's apps",
            self.name
        );
        ensure!(
            !self.name.trim().is_empty()
                && self.name.chars().count() <= 80
                && !self.description.trim().is_empty()
                && self.description.chars().count() <= 80
                && curated::CATEGORIES
                    .iter()
                    .any(|(id, _)| *id == self.category),
            "The app's name, description or category is invalid"
        );
        ensure!(
            !self.toolkit.is_empty()
                && self.toolkit.len() <= 100
                && self
                    .toolkit
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b)),
            "The app's library name is invalid"
        );
        ensure!(
            (1..=MAX_ACTIONS).contains(&self.actions.len()),
            "Choose 1 to {MAX_ACTIONS} actions"
        );
        let mut seen = std::collections::HashSet::new();
        for action in &self.actions {
            ensure!(
                action.id.starts_with(&format!("{}.", self.id)) && seen.insert(&action.id),
                "Each action needs its own name"
            );
            ensure!(
                ["read", "write"].contains(&action.effect.as_str())
                    && action.fields.len() <= MAX_FIELDS,
                "An action's review is invalid"
            );
        }
        Ok(())
    }

    /// The app in the catalog's shape, mapped for its one provider.
    pub(crate) fn catalog(&self) -> CatalogApp {
        CatalogApp {
            id: self.id.clone(),
            name: self.name.clone(),
            category: self.category.clone(),
            description: self.description.clone(),
            providers: BTreeMap::from([(self.provider.clone(), self.toolkit.clone())]),
            actions: self
                .actions
                .iter()
                .map(|a| Action {
                    id: a.id.clone(),
                    title: a.title.clone(),
                    effect: a.effect.clone(),
                    required: vec![],
                    providers: BTreeMap::from([(
                        self.provider.clone(),
                        own_names(&a.tool, a.fields.iter().map(String::as_str)),
                    )]),
                })
                .collect(),
            triggers: vec![],
            identity: BTreeMap::new(),
        }
    }
}

/// A binding whose fields keep the provider's names.
pub(crate) fn own_names<'a>(tool: &str, fields: impl Iterator<Item = &'a str>) -> Binding {
    Binding {
        tool: tool.to_owned(),
        fields: fields.map(|f| (f.to_owned(), f.to_owned())).collect(),
    }
}

/// The action and binding a saved operation of an added app was reviewed
/// against: its own tool, and exactly the fields its saved schema offers.
pub(crate) fn reviewed(
    operation: &crate::identity::control::model::Operation,
) -> (Action, Binding) {
    let fields = operation.schema["properties"]
        .as_object()
        .map(|p| p.keys().map(String::as_str).collect::<Vec<_>>())
        .unwrap_or_default();
    let binding = own_names(&operation.tool, fields.into_iter());
    let action = Action {
        id: operation.id.clone(),
        title: operation.title.clone(),
        effect: operation.effect.clone(),
        required: vec![],
        providers: BTreeMap::new(),
    };
    (action, binding)
}

/// Cuckoo's id for a library app: its library name in lower case, with anything
/// else as an underscore.
pub(crate) fn app_id(toolkit: &str) -> Result<String> {
    let id: String = toolkit
        .trim()
        .chars()
        .map(|c| match c.to_ascii_lowercase() {
            c @ ('a'..='z' | '0'..='9' | '_') => c,
            _ => '_',
        })
        .collect();
    crate::identity::identifier(&id)?;
    Ok(id)
}

/// Cuckoo's id for a library tool: the app, then the tool's name without the
/// app's own prefix (`HUBSPOT_CREATE_NOTE` is `hubspot.create_note`).
pub(crate) fn action_id(app: &str, toolkit: &str, tool: &str) -> String {
    let lower = tool.to_ascii_lowercase();
    let prefix = format!("{}_", toolkit.to_ascii_lowercase());
    let rest = lower.strip_prefix(&prefix).unwrap_or(&lower);
    format!("{app}.{rest}")
}

/// Up to [`MAX_FIELDS`] of a tool's fields Cuckoo can offer, required ones
/// first, or why the tool can't be offered. Only plain values and lists of
/// plain values are offered, as for built-in apps.
pub(crate) fn offered_fields(tool: &str, input: &Value) -> Result<Vec<String>, &'static str> {
    let properties = input["properties"]
        .as_object()
        .ok_or("It has no inputs Cuckoo can read.")?;
    let required: Vec<&str> = input["required"]
        .as_array()
        .map(|r| r.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    // Which fields are plain, found by offering each alone with nothing required.
    let mut alone = input.clone();
    alone["required"] = json!([]);
    let action = Action {
        id: "review.action".into(),
        title: "Review".into(),
        effect: "write".into(),
        required: vec![],
        providers: BTreeMap::new(),
    };
    let plain: Vec<&str> = properties
        .keys()
        .map(String::as_str)
        .filter(|name| {
            !name.is_empty()
                && name.len() <= 80
                && curated::project(&action, &own_names(tool, std::iter::once(*name)), &alone)
                    .is_some_and(|s| s["properties"].as_object().is_some_and(|p| !p.is_empty()))
        })
        .collect();
    if required.iter().any(|r| !plain.contains(r)) {
        return Err("It needs an input Cuckoo does not offer yet, such as a list of records.");
    }
    if required.len() > MAX_FIELDS {
        return Err("It needs more inputs than Cuckoo offers.");
    }
    let mut fields: Vec<String> = required.iter().map(|r| (*r).to_owned()).collect();
    for name in plain {
        if fields.len() == MAX_FIELDS {
            break;
        }
        if !fields.iter().any(|f| f == name) {
            fields.push(name.to_owned());
        }
    }
    Ok(fields)
}

/// Cuckoo's category for a library app, from the library's own category names.
pub(crate) fn category(library: &[&str]) -> &'static str {
    const WORDS: [(&str, &[&str]); 8] = [
        ("email_calendar", &["email", "calendar", "scheduling"]),
        ("files", &["document", "file", "storage", "notes"]),
        ("sales", &["crm", "sales", "marketing", "lead"]),
        (
            "accounting",
            &["accounting", "payment", "finance", "invoice"],
        ),
        (
            "messaging",
            &["communication", "messaging", "chat", "social"],
        ),
        (
            "projects",
            &["project", "task", "productivity", "developer"],
        ),
        ("support", &["support", "helpdesk", "customer service"]),
        ("shops", &["commerce", "shop", "store"]),
    ];
    for name in library {
        let name = name.to_lowercase();
        if let Some((id, _)) = WORDS
            .iter()
            .find(|(_, words)| words.iter().any(|w| name.contains(w)))
        {
            return id;
        }
    }
    "other"
}

/// A library description cut to what a tile shows: its first sentence, at most
/// 80 characters.
pub(crate) fn short(text: &str, fallback: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let sentence = text
        .split(". ")
        .next()
        .unwrap_or_default()
        .trim_end_matches('.');
    let text = if sentence.is_empty() {
        fallback
    } else {
        sentence
    };
    if text.chars().count() <= 80 {
        return text.to_owned();
    }
    let cut: String = text.chars().take(79).collect();
    format!("{}…", cut.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note() -> Value {
        json!({"type":"object","required":["body"],"properties":{
            "body":{"type":"string","description":"The note"},
            "contact_ids":{"type":"array","items":{"type":"string"}},
            "pinned":{"type":"boolean"},
            "properties":{"type":"object"},
            "records":{"type":"array","items":{"type":"object"}}}})
    }

    #[test]
    fn a_library_tool_offers_its_plain_fields_and_says_why_when_it_cannot() {
        assert_eq!(
            offered_fields("HUBSPOT_CREATE_NOTE", &note()).unwrap(),
            vec!["body", "contact_ids", "pinned"]
        );
        let mut nested = note();
        nested["required"] = json!(["records"]);
        assert!(
            offered_fields("X", &nested)
                .unwrap_err()
                .contains("does not offer")
        );
        assert!(offered_fields("X", &json!({})).is_err());
        assert_eq!(
            offered_fields("X", &json!({"type":"object","properties":{}})).unwrap(),
            Vec::<String>::new()
        );
        // Required fields come first when a tool has more than Cuckoo offers.
        let mut many = serde_json::Map::new();
        for n in 0..30 {
            many.insert(format!("f{n:02}"), json!({"type":"string"}));
        }
        let wide = json!({"type":"object","required":["f29"],"properties":many});
        let fields = offered_fields("X", &wide).unwrap();
        assert_eq!((fields.len(), fields[0].as_str()), (MAX_FIELDS, "f29"));
    }

    #[test]
    fn ids_names_and_categories_come_from_the_library_in_cuckoos_form() {
        assert_eq!(app_id("HubSpot").unwrap(), "hubspot");
        assert_eq!(app_id("docker-hub").unwrap(), "docker_hub");
        assert!(app_id("").is_err());
        assert_eq!(
            action_id("hubspot", "hubspot", "HUBSPOT_CREATE_NOTE"),
            "hubspot.create_note"
        );
        assert_eq!(action_id("x", "hubspot", "OTHER_TOOL"), "x.other_tool");
        assert_eq!(category(&["CRM", "Marketing Automation"]), "sales");
        assert_eq!(category(&["Weather"]), "other");
        assert_eq!(
            short("Customers and deals. More words.", "x"),
            "Customers and deals"
        );
        assert_eq!(short("  ", "HubSpot"), "HubSpot");
        assert_eq!(short(&"a".repeat(200), "x").chars().count(), 80);
    }

    #[test]
    fn an_added_app_is_a_catalog_app_and_a_saved_operation_carries_its_review() {
        let app = AddedApp {
            id: "hubspot".into(),
            name: "HubSpot".into(),
            description: "Customers and deals".into(),
            category: "sales".into(),
            provider: "composio".into(),
            toolkit: "hubspot".into(),
            actions: vec![AddedAction {
                id: "hubspot.create_note".into(),
                title: "Create note".into(),
                effect: "write".into(),
                tool: "HUBSPOT_CREATE_NOTE".into(),
                fields: vec!["body".into(), "pinned".into()],
            }],
        };
        app.validate().unwrap();
        let catalog = app.catalog();
        let action = &catalog.actions[0];
        let schema = curated::project(action, &action.providers["composio"], &note()).unwrap();
        assert_eq!(schema["required"], json!(["body"]));
        assert_eq!(schema["properties"].as_object().unwrap().len(), 2);
        // The saved operation alone gives the same review back.
        let operation = crate::identity::control::model::Operation {
            id: action.id.clone(),
            title: action.title.clone(),
            tool: "HUBSPOT_CREATE_NOTE".into(),
            effect: "write".into(),
            schema: schema.clone(),
            version: None,
        };
        operation.validate().unwrap();
        let (again, binding) = reviewed(&operation);
        assert_eq!(curated::project(&again, &binding, &note()), Some(schema));
        // A built-in app's id, or an action named for another app, is refused.
        let mut gmail = app.clone();
        gmail.id = "gmail".into();
        assert!(gmail.validate().is_err());
        let mut other = app.clone();
        other.actions[0].id = "slack.create_note".into();
        assert!(other.validate().is_err());
        let mut none = app;
        none.actions.clear();
        assert!(none.validate().is_err());
    }
}

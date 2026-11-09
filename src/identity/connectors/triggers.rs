//! App events under Cuckoo's names (the catalog's `triggers`), as a saved
//! connection's provider maps them. Event triggers refer to these ids, such as
//! `gmail.new_email`, never to a provider's trigger names (schema 109).
use super::curated;
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// One app event a connection's provider offers.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AppEvent {
    pub id: String,
    pub title: String,
    /// The app's name, for the source label people see.
    pub app_name: String,
    /// The provider's own trigger name (Composio's trigger slug).
    pub trigger: String,
    /// Cuckoo field name to the provider's payload field name.
    pub map: BTreeMap<String, String>,
    /// The provider trigger settings. Catalog events use the provider's
    /// defaults today, so every subscription of one event is the same.
    pub config: Value,
}
impl AppEvent {
    /// The fields people may filter on and agents receive, in order.
    pub fn fields(&self) -> Vec<String> {
        self.map.keys().cloned().collect()
    }
    pub fn public(&self) -> Value {
        json!({"id":self.id,"title":self.title,"fields":self.fields()})
    }
}

/// The events `provider` offers for the catalog app `app`.
pub(crate) fn events(app: &str, provider: &str) -> Vec<AppEvent> {
    let Some(catalog) = curated::app(app) else {
        return vec![];
    };
    catalog
        .triggers
        .iter()
        .filter_map(|t| {
            let binding = t.providers.get(provider)?;
            Some(AppEvent {
                id: t.id.clone(),
                title: t.title.clone(),
                app_name: catalog.name.clone(),
                trigger: binding.tool.clone(),
                map: binding.fields.clone(),
                config: json!({}),
            })
        })
        .collect()
}

pub(crate) fn event(app: &str, provider: &str, id: &str) -> Option<AppEvent> {
    events(app, provider).into_iter().find(|e| e.id == id)
}

/// The read action whose Allow lets an agent be started by this app's events:
/// the app's first reviewed read, in catalog order, that the saved account
/// offers. Event data is what that read could fetch, so the same rule that lets
/// the agent read the account lets the account's events reach it.
pub(crate) fn read_operation(app: &str, definition: &Value) -> Option<String> {
    let offered = definition["operations"].as_array()?;
    curated::app(app)?
        .actions
        .iter()
        .filter(|a| a.effect == "read")
        .find(|a| {
            offered
                .iter()
                .any(|o| o["id"] == a.id.as_str() && o["effect"] == "read")
        })
        .map(|a| a.id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_events_map_provider_fields_under_cuckoo_names() {
        let email = event("gmail", "composio", "gmail.new_email").unwrap();
        assert_eq!(email.trigger, "GMAIL_NEW_GMAIL_MESSAGE");
        assert_eq!(email.map["from"], "sender");
        assert!((1..=8).contains(&email.fields().len()));
        assert!(event("google_calendar", "composio", "google_calendar.new_event").is_some());
        assert!(event("gmail", "pipedream", "gmail.new_email").is_none());
        assert!(events("google_drive", "composio").is_empty());
        let definition = json!({"operations":[
            {"id":"gmail.send_email","effect":"write"},{"id":"gmail.find_emails","effect":"read"}]});
        assert_eq!(
            read_operation("gmail", &definition).as_deref(),
            Some("gmail.find_emails")
        );
        assert!(read_operation("gmail", &json!({"operations":[]})).is_none());
    }
}

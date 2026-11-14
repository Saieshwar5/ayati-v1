//! Event sources: operator profiles from the protected sources file, and
//! controller-owned provider subscriptions (schema 109). Both carry the provider
//! identity an event must match and the explicit fields it may pass on.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::PathBuf;
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Source {
    pub id: String,
    pub workspace: String,
    pub connection: String,
    pub connection_fingerprint: String,
    pub name: String,
    pub provider: String,
    pub user_id: String,
    pub connected_account_id: String,
    pub auth_config_id: String,
    pub trigger_id: String,
    pub trigger_slug: String,
    pub read_operation: String,
    pub fields: Vec<String>,
    pub secret_file: String,
    /// Set only for a provider subscription the controller made; the operator
    /// file can never set it. Its provider identity is then `user_id` (may be
    /// empty when the provider sends none), `connected_account_id`,
    /// `trigger_id` (the provider's subscription id) and `trigger_slug`.
    #[serde(skip)]
    pub subscribed: Option<Subscribed>,
}
/// What a provider subscription adds to a source.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Subscribed {
    /// The catalog event, such as `gmail.new_email`.
    pub event: String,
    /// Cuckoo field name to the provider's payload field name.
    pub map: BTreeMap<String, String>,
    /// Whether the subscription is active; a broken or removed one keeps its
    /// history readable but starts nothing new.
    pub active: bool,
}
impl Source {
    pub(crate) fn validate(&self) -> Result<()> {
        for id in [&self.id, &self.workspace, &self.connection] {
            crate::identity::automations::model::uuid(id)?;
        }
        ensure!(
            self.connection_fingerprint.len() == 43
                && match &self.subscribed {
                    None => self.provider == "composio_v3",
                    Some(s) => {
                        crate::identity::identifier(&self.provider).is_ok()
                            && s.map.keys().eq(self.fields.iter())
                    }
                },
            "Unsupported event provider"
        );
        ensure!(
            !self.name.trim().is_empty()
                && self.name.len() <= 160
                && !self.name.chars().any(char::is_control),
            "Invalid event-source name"
        );
        // A subscription's provider may not name a user, account or auth config.
        let optional = self.subscribed.is_some();
        for (value, required) in [
            (&self.user_id, !optional),
            (&self.connected_account_id, !optional),
            (&self.auth_config_id, !optional),
            (&self.trigger_id, true),
            (&self.trigger_slug, true),
            (&self.read_operation, true),
        ] {
            ensure!(
                (!required || !value.is_empty())
                    && value.len() <= 200
                    && !value.chars().any(char::is_control),
                "Invalid provider event identity"
            );
        }
        ensure!(
            !self.fields.is_empty() && self.fields.len() <= 8,
            "Review one to eight scalar event fields"
        );
        let mut fields = std::collections::HashSet::new();
        for field in &self.fields {
            crate::identity::identifier(field)?;
            ensure!(fields.insert(field), "Duplicate projected event field");
        }
        Ok(())
    }
    pub(crate) fn binding(&self) -> Value {
        let mut value = json!({"id":self.id,"workspace":self.workspace,"connection":self.connection,"connection_fingerprint":self.connection_fingerprint,"name":self.name,"provider":self.provider,"user_id":self.user_id,"connected_account_id":self.connected_account_id,"auth_config_id":self.auth_config_id,"trigger_id":self.trigger_id,"trigger_slug":self.trigger_slug,"read_operation":self.read_operation,"fields":self.fields});
        // Operator bindings keep their schema-28 shape, so saved triggers still match.
        if let Some(s) = &self.subscribed {
            value["event"] = json!(s.event);
            value["map"] = json!(s.map);
        }
        value
    }
    pub(crate) fn fingerprint(&self) -> Result<String> {
        let mut value = self.binding();
        value.sort_all_objects();
        Ok(crate::identity::provider::digest(&serde_json::to_string(
            &value,
        )?))
    }
    /// What people see. A subscription shows Cuckoo's event name, never the
    /// provider's trigger name.
    pub(crate) fn public(&self) -> Value {
        let event_type = match &self.subscribed {
            Some(s) => &s.event,
            None => &self.trigger_slug,
        };
        json!({"id":self.id,"name":self.name,"event_type":event_type,"fields":self.fields,"binding":self.fingerprint().ok()})
    }
    /// Whether new triggers may use this source and old ones resume: an
    /// operator source needs its signing secret, a subscription must be active.
    pub(crate) fn available(&self) -> Result<()> {
        match &self.subscribed {
            Some(s) => {
                ensure!(s.active, "This app event is no longer subscribed");
                Ok(())
            }
            None => self.secret().map(|_| ()),
        }
    }
    pub(crate) fn secret(&self) -> Result<String> {
        let secret =
            crate::identity::private_config::read(std::path::Path::new(&self.secret_file))?
                .trim()
                .to_owned();
        ensure!(
            secret.len() >= 16 && secret.len() <= 4096 && !secret.chars().any(char::is_control),
            "Event verification secret is unavailable"
        );
        Ok(secret)
    }
    pub(crate) fn project(&self, id: &str, event: &Value) -> Result<Value> {
        if let Some(subscribed) = &self.subscribed {
            if self.provider == "system" {
                return super::system_events::project(self, subscribed, id, event);
            }
            return super::provider_events::project(self, subscribed, id, event);
        }
        ensure!(
            event["id"] == id && event["type"] == "composio.trigger.message",
            "Unsupported event envelope"
        );
        let m = &event["metadata"];
        ensure!(
            m["user_id"] == self.user_id
                && m["connected_account_id"] == self.connected_account_id
                && m["auth_config_id"] == self.auth_config_id
                && m["trigger_id"] == self.trigger_id
                && m["trigger_slug"] == self.trigger_slug,
            "Event identity does not match its configured source"
        );
        let data = event["data"]
            .as_object()
            .context("Event data must be an object")?;
        let mut projected = serde_json::Map::new();
        for field in &self.fields {
            let value = data
                .get(field)
                .context("A reviewed event field is missing")?;
            ensure!(
                value.is_null()
                    || value.is_boolean()
                    || value
                        .as_str()
                        .is_some_and(|v| v.len() <= 2000 && !v.contains('\0'))
                    || value
                        .as_f64()
                        .is_some_and(|n| n.is_finite() && n.abs() <= 9_007_199_254_740_991.0),
                "Projected event fields must be bounded scalars"
            );
            projected.insert(field.clone(), value.clone());
        }
        let value = Value::Object(projected);
        ensure!(
            serde_json::to_vec(&value)?.len() <= 6144,
            "Projected event data exceeds 6 KiB"
        );
        Ok(value)
    }
}
static FILE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
/// Set once at controller startup; the file itself is reread on each use so
/// reviewed operator edits apply without a restart.
pub(crate) fn configure(path: Option<PathBuf>) {
    if let Some(path) = path {
        let _ = FILE.set(path);
    }
}
fn path() -> Option<PathBuf> {
    if let Some(path) = FILE.get() {
        return Some(path.clone());
    }
    #[cfg(test)]
    if let Some(path) = super::test_catalog::path() {
        return Some(path);
    }
    None
}
pub(crate) fn sources(w: &str) -> Result<Vec<Source>> {
    let Some(path) = path() else {
        return Ok(vec![]);
    };
    let all: Vec<Source> = serde_json::from_str(&crate::identity::private_config::read(&path)?)
        .map_err(|_| anyhow::anyhow!("Invalid private event-source configuration"))?;
    ensure!(all.len() <= 50, "Too many configured event sources");
    let mut ids = std::collections::HashSet::new();
    for source in &all {
        source.validate()?;
        ensure!(
            ids.insert((&source.workspace, &source.id)),
            "Duplicate event-source identity"
        );
    }
    Ok(all
        .into_iter()
        .filter(|source| source.workspace == w)
        .collect())
}
pub(crate) fn source(w: &str, id: &str) -> Result<Source> {
    sources(w)?
        .into_iter()
        .find(|source| source.id == id)
        .context("Event source is unavailable")
}

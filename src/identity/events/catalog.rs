//! Provider identity and explicit projections come from protected operator input.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
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
}
impl Source {
    pub(crate) fn validate(&self) -> Result<()> {
        for id in [&self.id, &self.workspace, &self.connection] {
            crate::identity::automations::model::uuid(id)?;
        }
        ensure!(
            self.provider == "composio_v3" && self.connection_fingerprint.len() == 43,
            "Unsupported event provider"
        );
        ensure!(
            !self.name.trim().is_empty()
                && self.name.len() <= 160
                && !self.name.chars().any(char::is_control),
            "Invalid event-source name"
        );
        for value in [
            &self.user_id,
            &self.connected_account_id,
            &self.auth_config_id,
            &self.trigger_id,
            &self.trigger_slug,
            &self.read_operation,
        ] {
            ensure!(
                !value.is_empty() && value.len() <= 200 && !value.chars().any(char::is_control),
                "Invalid provider event identity"
            );
        }
        ensure!(
            !self.fields.is_empty() && self.fields.len() <= 8,
            "Review one to eight scalar event fields"
        );
        let mut fields = std::collections::HashSet::new();
        for field in &self.fields {
            crate::platform::identifier(field)?;
            ensure!(fields.insert(field), "Duplicate projected event field");
        }
        Ok(())
    }
    pub(crate) fn binding(&self) -> Value {
        json!({"id":self.id,"workspace":self.workspace,"connection":self.connection,"connection_fingerprint":self.connection_fingerprint,"name":self.name,"provider":self.provider,"user_id":self.user_id,"connected_account_id":self.connected_account_id,"auth_config_id":self.auth_config_id,"trigger_id":self.trigger_id,"trigger_slug":self.trigger_slug,"read_operation":self.read_operation,"fields":self.fields})
    }
    pub(crate) fn fingerprint(&self) -> Result<String> {
        let mut value = self.binding();
        value.sort_all_objects();
        Ok(crate::identity::provider::digest(&serde_json::to_string(
            &value,
        )?))
    }
    pub(crate) fn public(&self) -> Value {
        json!({"id":self.id,"name":self.name,"event_type":self.trigger_slug,"fields":self.fields,"binding":self.fingerprint().ok()})
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
fn path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("AYATI_EVENT_SOURCES_FILE") {
        return Some(path.into());
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

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Trigger {
    pub id: String,
    pub source: String,
    pub binding: String,
    /// Where to also post a short result, if anywhere. Runs happen in a private
    /// work room (schema 115); older triggers keep their conversation here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat: Option<String>,
    /// `fresh` (the default) or `remember`: what a run sees of earlier runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory: Option<String>,
    pub name: String,
    pub instructions: String,
    pub connection_revision: i64,
    pub read_revision: i64,
    pub delegation_revision: Option<i64>,
    pub filter: Value,
}
impl Trigger {
    pub(crate) fn validate(&self) -> Result<()> {
        for id in [Some(&self.id), Some(&self.source), self.chat.as_ref()]
            .into_iter()
            .flatten()
        {
            crate::identity::automations::model::uuid(id)?;
        }
        ensure!(
            self.memory
                .as_deref()
                .is_none_or(|m| crate::identity::postgres::MEMORY.contains(&m)),
            "Choose what the automation remembers: fresh or remember"
        );
        ensure!(
            self.binding.len() == 43
                && self.connection_revision > 0
                && self.read_revision > 0
                && self.delegation_revision.is_none_or(|r| r > 0),
            "Invalid event-source version"
        );
        ensure!(
            !self.name.trim().is_empty()
                && self.name.len() <= 100
                && !self.name.chars().any(char::is_control),
            "Use a trigger name up to 100 bytes"
        );
        ensure!(
            !self.instructions.trim().is_empty()
                && self.instructions.len() <= 4000
                && !self.instructions.contains('\0'),
            "Use event-task instructions up to 4,000 bytes"
        );
        let filter = self
            .filter
            .as_object()
            .context("Use exact scalar event filters")?;
        ensure!(
            filter.len() <= 8 && self.filter.to_string().len() <= 1000,
            "Event filters exceed their supported size"
        );
        for (key, value) in filter {
            crate::identity::identifier(key)?;
            ensure!(
                !value.is_array() && !value.is_object(),
                "Use exact scalar event filters"
            );
        }
        Ok(())
    }
}
/// A trigger started by a connected app's event (schema 109). In place of a
/// source the person names the connection and a catalog event; the controller
/// finds or creates the provider subscription that becomes the source.
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FromApp {
    pub id: String,
    pub connection: String,
    pub event: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory: Option<String>,
    pub name: String,
    pub instructions: String,
    pub connection_revision: i64,
    pub read_revision: i64,
    pub delegation_revision: Option<i64>,
    pub filter: Value,
}
impl FromApp {
    /// The trigger saved for the subscription `source` with fingerprint `binding`.
    pub(crate) fn trigger(&self, source: &str, binding: &str) -> Trigger {
        Trigger {
            id: self.id.clone(),
            source: source.to_owned(),
            binding: binding.to_owned(),
            chat: self.chat.clone(),
            memory: self.memory.clone(),
            name: self.name.clone(),
            instructions: self.instructions.clone(),
            connection_revision: self.connection_revision,
            read_revision: self.read_revision,
            delegation_revision: self.delegation_revision,
            filter: self.filter.clone(),
        }
    }
    pub(crate) fn validate(&self) -> Result<()> {
        crate::identity::automations::model::uuid(&self.connection)?;
        ensure!(
            self.event.len() <= 129
                && self.event.split_once('.').is_some_and(|(app, name)| {
                    [app, name].iter().all(|part| {
                        !part.is_empty()
                            && part
                                .bytes()
                                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
                    })
                }),
            "Choose one of the app's events"
        );
        self.trigger(&self.connection, &"a".repeat(43)).validate()
    }
}
pub(crate) fn fingerprint(value: &impl Serialize) -> Result<String> {
    let mut value = serde_json::to_value(value)?;
    value.sort_all_objects();
    Ok(crate::identity::provider::digest(&serde_json::to_string(
        &value,
    )?))
}
pub(crate) fn matches(filter: &Value, data: &Value) -> bool {
    filter
        .as_object()
        .is_some_and(|fields| fields.iter().all(|(k, v)| data.get(k) == Some(v)))
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Change {
    pub request: String,
    pub revision: i64,
    pub enabled: bool,
    pub connection_revision: Option<i64>,
    pub read_revision: Option<i64>,
    pub delegation_revision: Option<i64>,
    /// Remove the trigger for good (schema 109); `enabled` must be false.
    /// Left out when false, so requests saved before schema 109 still match.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub remove: bool,
}
impl Change {
    pub(crate) fn validate(&self) -> Result<()> {
        crate::identity::automations::model::uuid(&self.request)?;
        ensure!(self.revision > 0, "Invalid event-trigger revision");
        Ok(())
    }
}
#[derive(Debug)]
pub(crate) struct InboxFull;
impl std::fmt::Display for InboxFull {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Event intake is at its operational limit; retry delivery later"
        )
    }
}
impl std::error::Error for InboxFull {}

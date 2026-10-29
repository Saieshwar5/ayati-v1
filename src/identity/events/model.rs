use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Trigger {
    pub id: String,
    pub source: String,
    pub binding: String,
    pub chat: String,
    pub name: String,
    pub instructions: String,
    pub connection_revision: i64,
    pub read_revision: i64,
    pub delegation_revision: Option<i64>,
    pub filter: Value,
}
impl Trigger {
    pub(crate) fn validate(&self) -> Result<()> {
        for id in [&self.id, &self.source, &self.chat] {
            crate::identity::automations::model::uuid(id)?;
        }
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

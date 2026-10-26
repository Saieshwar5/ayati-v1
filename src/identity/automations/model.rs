use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Schedule {
    pub id: String,
    pub chat: String,
    pub name: String,
    pub content: String,
    pub first_ms: i64,
    pub interval_seconds: Option<i64>,
    pub timezone: String,
}
impl Schedule {
    pub(crate) fn validate(&self) -> Result<()> {
        uuid(&self.id)?;
        uuid(&self.chat)?;
        ensure!(
            !self.name.trim().is_empty()
                && self.name.len() <= 100
                && !self.name.chars().any(char::is_control),
            "Use a routine name up to 100 bytes"
        );
        ensure!(
            !self.content.trim().is_empty()
                && self.content.len() <= 10000
                && !self.content.contains('\0'),
            "Use task instructions up to 10,000 bytes"
        );
        ensure!(
            self.first_ms > 0
                && self.first_ms <= 4102444800000
                && self
                    .interval_seconds
                    .is_none_or(|v| (1..=31536000).contains(&v)),
            "Choose a supported first run and fixed interval"
        );
        ensure!(
            !self.timezone.is_empty()
                && self.timezone.len() <= 100
                && !self.timezone.chars().any(char::is_control),
            "Choose a timezone label up to 100 bytes"
        );
        Ok(())
    }
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Change {
    pub request: String,
    pub revision: i64,
    pub enabled: bool,
}
impl Change {
    pub(crate) fn validate(&self) -> Result<()> {
        uuid(&self.request)?;
        ensure!(self.revision > 0, "Invalid routine revision");
        Ok(())
    }
}
pub(crate) fn uuid(id: &str) -> Result<()> {
    ensure!(
        uuid::Uuid::parse_str(id).is_ok(),
        "Invalid routine identity"
    );
    Ok(())
}
pub(crate) fn fingerprint(input: &Schedule) -> Result<String> {
    let mut value = serde_json::to_value(input)?;
    value.sort_all_objects();
    Ok(crate::identity::provider::digest(&serde_json::to_string(
        &value,
    )?))
}
pub(crate) fn due(first: i64, interval: Option<i64>, now: i64) -> (i64, Option<i64>) {
    match interval {
        Some(seconds) => {
            let step = seconds * 1000;
            let at = first + ((now - first).max(0) / step) * step;
            (at, at.checked_add(step))
        }
        None => (first, None),
    }
}

//! Versioned local machine protocol. Secrets deliberately have no Debug implementation.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Scope {
    pub executor: String,
    pub incarnation: String,
    pub slot: String,
    #[serde(default, skip_serializing_if = "zero")]
    pub slot_generation: i64,
    pub assignment: String,
    pub workspace: String,
    pub agent: String,
    pub chat: String,
    pub thread: Option<String>,
    pub reply: String,
    pub generation: i32,
}
fn zero(value: &i64) -> bool {
    *value == 0
}
impl Scope {
    pub fn resource(&self) -> String {
        let resource = format!(
            "control:{}:{}:{}",
            self.executor, self.incarnation, self.slot
        );
        if self.slot_generation == 0 {
            resource
        } else {
            format!("{resource}:{}", self.slot_generation)
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Registration {
    pub executor: String,
    pub incarnation: String,
    pub token: String,
    #[serde(default)]
    pub pooled: bool,
}
impl Registration {
    pub fn validate(&self) -> Result<()> {
        uuid(&self.executor)?;
        uuid(&self.incarnation)?;
        token(&self.token)
    }
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Reservation {
    pub request: String,
    pub token: String,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AssignedRequest {
    pub scope: Scope,
    pub session: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Completion {
    pub assignment: AssignedRequest,
    pub state: String,
    pub text: String,
}
impl Completion {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            ["complete", "failed"].contains(&self.state.as_str()),
            "Invalid outcome"
        );
        ensure!(
            self.text.len() <= 65536 && !self.text.contains('\0'),
            "Invalid result size/content"
        );
        Ok(())
    }
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Release {
    pub scope: Scope,
    pub reason: String,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Launch {
    pub url: String,
    pub scope: Scope,
    pub token: String,
}

pub(crate) fn uuid(value: &str) -> Result<()> {
    ensure!(
        uuid::Uuid::parse_str(value).is_ok_and(|id| id.to_string() == value),
        "Invalid identity"
    );
    Ok(())
}
pub(crate) fn token(value: &str) -> Result<()> {
    ensure!(
        value.len() == 43
            && value
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c)),
        "Invalid machine credential"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_scope_omits_slot_generation_to_preserve_completion_fingerprints() {
        let old = serde_json::json!({"executor":"e","incarnation":"i","slot":"s","assignment":"a","workspace":"w","agent":"agent","chat":"c","thread":null,"reply":"r","generation":1});
        let scope: Scope = serde_json::from_value(old.clone()).unwrap();
        assert_eq!(scope.slot_generation, 0);
        assert_eq!(serde_json::to_value(&scope).unwrap(), old);
        assert_eq!(scope.resource(), "control:e:i:s");
    }
}

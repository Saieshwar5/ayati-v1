//! Versioned local machine protocol. Secrets deliberately have no Debug implementation.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Scope {
    pub executor: String,
    pub incarnation: String,
    pub slot: String,
    pub assignment: String,
    pub workspace: String,
    pub agent: String,
    pub chat: String,
    pub thread: Option<String>,
    pub reply: String,
    pub generation: i32,
}
impl Scope {
    pub fn resource(&self) -> String {
        format!(
            "control:{}:{}:{}",
            self.executor, self.incarnation, self.slot
        )
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Registration {
    pub executor: String,
    pub incarnation: String,
    pub token: String,
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

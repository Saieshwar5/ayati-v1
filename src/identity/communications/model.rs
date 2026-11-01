//! Transport data is evidence, never a requester-selected workspace or principal.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

/// Cuckoo Chat and the local email/WhatsApp simulations. Live transports are
/// adapters named by a string (`adapters::TransportAdapter`).
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Transport {
    Chat,
    Email,
    Whatsapp,
}
impl Transport {
    pub fn name(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Email => "email",
            Self::Whatsapp => "whatsapp",
        }
    }
    pub fn capabilities(self) -> serde_json::Value {
        serde_json::json!({"text":true,"streaming":self==Self::Chat,
            "attachments":self==Self::Chat,"external_delivery":false,
            "simulation_only":self!=Self::Chat})
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Message {
    pub event: String,
    pub sender: String,
    pub recipient: String,
    pub conversation: String,
    pub text: String,
    #[serde(default)]
    pub attachments: Vec<String>,
    #[serde(default)]
    pub automated: bool,
    /// A thread inside the platform conversation, where the platform has them.
    /// Left out when absent, so earlier envelopes keep their fingerprints.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread: Option<String>,
}
impl Message {
    pub fn validate(&self) -> Result<()> {
        for field in [
            &self.event,
            &self.sender,
            &self.recipient,
            &self.conversation,
        ]
        .into_iter()
        .chain(&self.thread)
        {
            ensure!(
                !field.trim().is_empty()
                    && field.len() <= 320
                    && !field.chars().any(char::is_control),
                "Invalid communication identifier"
            );
        }
        ensure!(
            !self.text.trim().is_empty() && self.text.len() <= 12000,
            "Message is empty or too long"
        );
        ensure!(self.attachments.len() <= 8, "Too many attachments");
        Ok(())
    }
    pub fn fingerprint(&self) -> Result<String> {
        Ok(crate::identity::provider::digest(&serde_json::to_string(
            self,
        )?))
    }
    /// The room this message belongs to: one per platform conversation and thread.
    pub fn room(&self) -> String {
        match &self.thread {
            Some(thread) => format!("{}#{thread}", self.conversation),
            None => self.conversation.clone(),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Endpoint {
    pub id: String,
    pub agent: String,
    pub transport: Transport,
    pub address: String,
}
/// An agent's address on a live transport, for example a bot a developer's
/// transport server runs. The server must list the address before it is saved.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LiveEndpoint {
    pub id: String,
    pub agent: String,
    pub transport: String,
    pub server: String,
    pub address: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Binding {
    pub sender: String,
    pub user: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SetState {
    pub revision: i64,
    pub enabled: bool,
}
#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Outcome {
    Accepted,
    Delivered,
    Rejected,
    Timeout,
    Crash,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Dispatch {
    pub outcome: Outcome,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Receipt {
    pub attempt: String,
    pub provider_id: String,
    pub delivered: bool,
}

//! The transport contract. Each live transport is one adapter: it reads new
//! events from its platform, sends a reply and says what became of a send.
//! Everything else is shared and never rewritten per transport: rooms, who may
//! write, context, approvals, the outbox and audit.
//!
//! Adding a transport is one module implementing [`TransportAdapter`], one line
//! in [`configure`] and passing `conformance_tests`. Cuckoo Chat and the local
//! email/WhatsApp simulations keep their own paths and are not adapters.
use super::model::Message;
use anyhow::{Context, Result};
use serde::Serialize;
use std::sync::{Arc, OnceLock};

/// One place the gateway reads from: a transport server serving one workspace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Source {
    pub workspace: String,
    pub transport: String,
    pub server: String,
}

/// Where a reply goes: the endpoint's server and its address on the platform.
#[derive(Clone, Debug)]
pub(crate) struct Route {
    pub workspace: String,
    pub server: String,
    pub address: String,
}

/// One reply to send. `key` identifies this attempt: a platform must never send
/// twice for the same key. Cuckoo, not the agent, chooses every field.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct Outgoing {
    pub key: String,
    pub to: String,
    pub conversation: String,
    pub thread: Option<String>,
    /// Markdown. The platform's bridge converts or strips formatting.
    pub text: String,
    /// Email only: the thread and subject to answer under. Never sent to a
    /// developer's transport server.
    #[serde(skip)]
    pub mail: Option<MailReply>,
}

/// What an email reply needs beyond its text, saved when the mail arrived.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct MailReply {
    /// The agent's name, shown as the sender.
    pub from_name: String,
    pub subject: String,
    /// The person's mail being answered, without brackets.
    pub in_reply_to: Option<String>,
    /// The thread so far, oldest first, ending with `in_reply_to`.
    pub references: Vec<String>,
}

pub(crate) enum Event {
    Message(Message),
    Receipt { key: String, delivered: bool },
}

/// New events, oldest first, and the cursor to read from next time.
pub(crate) struct Batch {
    pub events: Vec<Event>,
    pub next: Option<String>,
    /// Malformed or unsupported events (group conversations in v1), dropped.
    pub skipped: usize,
}

#[derive(Debug, PartialEq)]
pub(crate) enum Sent {
    /// The platform took it; delivery may be confirmed later.
    Accepted(Option<String>),
    Delivered(Option<String>),
    /// The platform said no. Nothing was sent.
    Refused(String),
    /// The platform was never reached. Safe to try again later.
    Unreached(String),
    /// It may have been sent. Never resent; asked about again later.
    Uncertain(String),
}

/// A transport server an adapter bridges for a workspace, as shown to admins.
#[derive(Serialize)]
pub(crate) struct Server {
    pub id: String,
    pub name: String,
}

#[async_trait::async_trait]
pub(crate) trait TransportAdapter: Send + Sync {
    /// Saved on endpoints, for example `mcp`.
    fn name(&self) -> &'static str;
    /// The servers this adapter can bridge for `workspace`.
    fn servers(&self, workspace: &str) -> Result<Vec<Server>>;
    /// Confirms `route` is an address the platform serves, before it is saved.
    async fn check(&self, route: &Route) -> Result<()>;
    /// Events after `cursor`.
    async fn fetch(&self, source: &Source, cursor: Option<&str>) -> Result<Batch>;
    /// Never errors: every failure says whether the message could have gone.
    async fn send(&self, route: &Route, outgoing: &Outgoing) -> Sent;
    /// What became of an unclear send; `Uncertain` while the platform can't say.
    async fn status(&self, route: &Route, key: &str) -> Sent;
}

/// Process-wide transport settings, read once at controller start.
pub(crate) struct Settings {
    pub transport_servers_file: Option<std::path::PathBuf>,
    pub email_file: Option<std::path::PathBuf>,
}
static ADAPTERS: OnceLock<Vec<Arc<dyn TransportAdapter>>> = OnceLock::new();

/// Registers the live transports this controller offers.
pub(crate) fn configure(settings: Settings) {
    super::email::settings::configure(settings.email_file);
    let adapters: Vec<Arc<dyn TransportAdapter>> = vec![
        Arc::new(super::mcp_transport::McpTransport::new(
            settings.transport_servers_file,
        )),
        Arc::new(super::email::ses::EmailTransport::new()),
    ];
    let _ = ADAPTERS.set(adapters);
}

pub(crate) fn settings(config: &crate::config::Controller) -> Settings {
    Settings {
        transport_servers_file: config.transport_servers_file.clone(),
        email_file: config.email_file.clone(),
    }
}

pub(crate) fn all() -> Vec<Arc<dyn TransportAdapter>> {
    ADAPTERS.get().cloned().unwrap_or_default()
}

pub(crate) fn named(name: &str) -> Result<Arc<dyn TransportAdapter>> {
    all()
        .into_iter()
        .find(|a| a.name() == name)
        .context("This transport is not configured on this server")
}

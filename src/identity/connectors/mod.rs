//! Connector providers: the plug-in boundary between Cuckoo's control layer and
//! whoever signs in to an app and calls it.
//!
//! Cuckoo owns what people see and decide: the catalog, which apps admins allow,
//! connected accounts, rules, approvals and receipts. A provider only signs an
//! account in, keeps its tokens and makes the calls. Every provider does the same
//! jobs ([`ConnectorProvider`]); adding one is a module that implements them plus
//! one line in [`configure`]. Agents never see a provider, its links or its keys.
//!
//! Providers today: operator-configured remote MCP servers (`mcp`) and Composio
//! (`composio`). The provider is chosen per app, so one app can move to another
//! provider, or to a connector Cuckoo builds itself, without touching the others.
mod composio;
mod composio_api;
#[cfg(test)]
mod conformance_tests;
pub(crate) mod curated;
#[cfg(test)]
pub(crate) mod fake_composio;
#[cfg(test)]
mod fake_mcp;
mod mcp_servers;
pub(crate) mod routes;

use crate::identity::control::model::Operation;
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::sync::{Arc, OnceLock};

/// An app people can allow and connect, as Cuckoo shows it.
pub(crate) struct App {
    pub id: String,
    pub name: String,
    /// A label for the account, when the provider knows it before connecting.
    pub account: Option<String>,
    pub operations: Vec<Operation>,
    pub provider: &'static str,
}
impl App {
    /// The shape the Connections page lists, without provider details.
    pub fn public(&self) -> Value {
        json!({"id":self.id,"name":self.name,"account":self.account,"provider":self.provider,
            "operations":self.operations.iter().map(|o| json!({"id":o.id,"title":o.title,"effect":o.effect})).collect::<Vec<_>>()})
    }
}

/// A verified account, ready to save: the reviewed definition rules are set
/// against, the provider's own identity for it, and a fingerprint that changes
/// whenever either does.
pub(crate) struct Account {
    pub definition: Value,
    pub binding: Option<Value>,
    pub fingerprint: String,
}

/// The first step of connecting an app.
pub(crate) enum Connect {
    /// Verified at once (accounts an operator configured).
    Ready(Account),
    /// The person must sign in at `url`; the provider returns them to the
    /// callback, and [`ConnectorProvider::finish`] completes the account.
    SignIn {
        binding: Value,
        url: String,
        expires: i64,
    },
}

/// A saved connection, as the gateway hands it to its provider.
pub(crate) struct Saved<'a> {
    pub workspace: &'a str,
    pub server: &'a str,
    pub binding: Option<&'a Value>,
    pub fingerprint: &'a str,
}

/// What happened when an action ran.
pub(crate) enum Outcome {
    Completed(Value),
    /// The provider did not confirm the result: a write may or may not have
    /// happened, so it is never retried automatically.
    Uncertain,
}

/// Why an action was not sent. Nothing reached the app.
#[derive(Debug)]
pub(crate) enum Refusal {
    /// The account needs signing in again (expired, revoked or disabled).
    NeedsReconnect,
    /// The account or the action no longer matches what was reviewed.
    Changed,
}
impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NeedsReconnect => "The account needs to be reconnected. An admin can reconnect it in Settings, Connections. Nothing was sent.",
            Self::Changed => "This action changed since it was reviewed. An admin needs to reconnect and review the account. Nothing was sent.",
        })
    }
}
impl std::error::Error for Refusal {}

/// An action checked and ready to send.
#[async_trait::async_trait]
pub(crate) trait Call: Send {
    async fn run(self: Box<Self>, arguments: &Value) -> Outcome;
}

/// The jobs every connector provider does.
#[async_trait::async_trait]
pub(crate) trait ConnectorProvider: Send + Sync {
    /// Stored on each connection; never shown to agents.
    fn name(&self) -> &'static str;
    /// Apps this provider offers the workspace.
    fn apps(&self, workspace: &str) -> Result<Vec<App>>;
    /// Start connecting `app`. `callback` is where a sign-in page returns.
    async fn connect(&self, workspace: &str, app: &str, callback: &str) -> Result<Connect>;
    /// After the person signed in: check the account and build what is saved.
    async fn finish(&self, workspace: &str, app: &str, binding: &Value) -> Result<Account>;
    /// Remove the account at the provider and revoke its access. Cuckoo has
    /// already stopped using it; this cleans up the provider's side.
    async fn disconnect(&self, workspace: &str, app: &str, binding: Option<&Value>) -> Result<()>;
    /// Check the saved account still works and `operation` still means what was
    /// reviewed. Nothing is sent to the app. A [`Refusal`] explains a refusal.
    async fn prepare(&self, saved: &Saved<'_>, operation: &Operation) -> Result<Box<dyn Call>>;
}

/// Process-wide provider settings, read once at controller start.
pub(crate) struct Settings {
    pub mcp_servers_file: Option<std::path::PathBuf>,
    pub composio: Option<composio::Settings>,
}
static PROVIDERS: OnceLock<Vec<Arc<dyn ConnectorProvider>>> = OnceLock::new();

/// Registers the providers this controller offers. The operator MCP file is
/// always a provider; Composio only when its key is configured.
pub(crate) fn configure(settings: Settings) {
    let mut providers: Vec<Arc<dyn ConnectorProvider>> = vec![Arc::new(
        mcp_servers::McpServers::new(settings.mcp_servers_file),
    )];
    if let Some(composio) = settings.composio {
        match composio::Composio::new(composio) {
            Ok(provider) => providers.push(Arc::new(provider)),
            Err(error) => tracing::warn!("Composio connectors are unavailable: {error}"),
        }
    }
    let _ = PROVIDERS.set(providers);
}

/// Reads the Composio settings from the controller configuration.
pub(crate) fn settings(config: &crate::config::Controller) -> Settings {
    Settings {
        mcp_servers_file: config.mcp_servers_file.clone(),
        composio: composio::Settings::from_config(config),
    }
}

fn providers() -> Vec<Arc<dyn ConnectorProvider>> {
    PROVIDERS.get().cloned().unwrap_or_else(|| {
        vec![Arc::new(mcp_servers::McpServers::new(
            crate::identity::control::catalog::file(),
        ))]
    })
}

pub(crate) fn named(name: &str) -> Result<Arc<dyn ConnectorProvider>> {
    providers()
        .into_iter()
        .find(|p| p.name() == name)
        .context("This connector provider is not configured on this server")
}

/// Every app offered to the workspace, across providers. One app id never
/// belongs to two providers at once.
pub(crate) fn apps(workspace: &str) -> Result<Vec<App>> {
    let mut apps: Vec<App> = vec![];
    for provider in providers() {
        for app in provider.apps(workspace)? {
            ensure!(
                apps.iter().all(|a| a.id != app.id),
                "Two connector providers offer {}",
                app.id
            );
            apps.push(app);
        }
    }
    Ok(apps)
}

/// The provider serving `app` for this workspace.
pub(crate) fn provider_for(workspace: &str, app: &str) -> Result<Arc<dyn ConnectorProvider>> {
    for provider in providers() {
        if provider.apps(workspace)?.iter().any(|a| a.id == app) {
            return Ok(provider);
        }
    }
    anyhow::bail!("This connector is not offered on this server")
}

/// A Composio provider against a local fake, for tests outside this module.
#[cfg(test)]
pub(crate) fn composio_for_tests(fake: &fake_composio::FakeComposio) -> Arc<dyn ConnectorProvider> {
    Arc::new(
        composio::Composio::new(composio::Settings {
            base: fake.base.clone(),
            key_file: fake.key_file.clone(),
            auth_configs: Default::default(),
        })
        .unwrap(),
    )
}

/// A fingerprint of everything that defines a saved account.
pub(crate) fn fingerprint(provider: &str, definition: &Value, binding: Option<&Value>) -> String {
    crate::identity::provider::digest(&format!(
        "{provider}\n{definition}\n{}",
        binding.cloned().unwrap_or(Value::Null)
    ))
}

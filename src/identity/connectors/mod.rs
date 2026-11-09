//! Connector providers: the plug-in boundary between Cuckoo's control layer and
//! whoever signs in to an app and calls it.
//!
//! Cuckoo owns what people see and decide: the catalog, which apps admins allow,
//! connected accounts, rules, approvals and receipts. A provider only signs an
//! account in, keeps its tokens and makes the calls. Every provider does the same
//! jobs ([`ConnectorProvider`]); adding one is a module that implements them plus
//! one line in [`configure`]. Agents never see a provider, its links or its keys.
//!
//! Providers today: operator-configured remote MCP servers (`mcp`), Composio
//! (`composio`) and Pipedream Connect (`pipedream`). The provider is chosen per
//! app and workspace ([`routing`]), so one app can move to another provider, or
//! to a connector Cuckoo builds itself, without touching the others.
mod composio;
mod composio_api;
#[cfg(test)]
mod conformance_tests;
pub(crate) mod connect;
pub(crate) mod curated;
pub(crate) mod drift;
#[cfg(test)]
pub(crate) mod fake_composio;
#[cfg(test)]
mod fake_mcp;
#[cfg(test)]
pub(crate) mod fake_pipedream;
mod mcp_servers;
mod pipedream;
mod pipedream_accounts;
mod pipedream_api;
mod pipedream_events;
mod pipedream_props;
#[cfg(test)]
mod pipedream_sign_in_tests;
#[cfg(test)]
mod pipedream_tests;
pub(crate) mod pricing;
pub(crate) mod providers_file;
pub(crate) mod routes;
pub(crate) mod routing;

use crate::identity::control::model::Operation;
use crate::identity::postgres::PgStore;
use anyhow::{Context, Result};
use providers_file::ProvidersFile;
use serde_json::{Value, json};
use std::path::PathBuf;
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
    /// The app's own stable identity for the account (a mailbox, a Slack team
    /// and user), when the provider can prove it. Reconnecting the same
    /// identity keeps the account's rules; `None` always starts over.
    pub identity: Option<String>,
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
    /// `key` is the same for every attempt at one action (its id): providers
    /// that support idempotency pass it on so the app never acts twice.
    async fn run(self: Box<Self>, arguments: &Value, key: &str) -> Outcome;
}

/// What a provider supports. Screens and the gateway read these flags, never a
/// provider's name, so a new provider or connector kind needs no screen changes.
#[derive(Clone, Copy, Debug, Default, serde::Serialize)]
pub(crate) struct Capabilities {
    /// Accounts connect through a sign-in page.
    pub sign_in: bool,
    /// Disconnecting revokes access at the provider.
    pub revoke: bool,
    /// Actions may take lists of plain values (cc, attendees).
    pub list_fields: bool,
    /// The call's key reaches the app, so a repeated call can't act twice.
    pub idempotency_key: bool,
    /// [`ConnectorProvider::find_outcome`] can answer for some actions.
    pub find_outcome: bool,
    /// Events can be subscribed to (see the trigger methods).
    pub triggers: bool,
}

/// Whether an unconfirmed write happened, as far as the provider can tell.
#[derive(Debug, PartialEq)]
pub(crate) enum Found {
    Happened(Value),
    NotHappened,
    Unknown,
}

/// A provider subscription for one event type on one account.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Subscription {
    /// The provider's id for it (a Composio `ti_…`, a Pipedream `dc_…`).
    pub id: String,
    /// A per-subscription signing key, when the provider issues one. The caller
    /// seals it before saving; it is never logged or sent to a client.
    pub signing_key: Option<String>,
}

/// One verified delivery, in Cuckoo's terms. Signature and time window are
/// checked before this exists; the caller still matches it to a saved
/// subscription and workspace before trusting any field.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Delivery {
    /// The provider's event id: the receipt key, so a retry is a duplicate.
    pub event_id: String,
    /// The provider's identity for the workspace (Composio's user id), if sent.
    pub user: Option<String>,
    /// The provider's account id, if the event names one.
    pub account: Option<String>,
    pub kind: DeliveryKind,
}
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum DeliveryKind {
    /// An app event for the subscription `subscription`.
    Event {
        subscription: String,
        trigger: String,
        data: Value,
    },
    /// The account's sign-in stopped working at the provider.
    AccountExpired,
    /// Something the provider sends that Cuckoo doesn't use; acknowledged only.
    Ignored,
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

    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }
    /// The provider's current input schema for a reviewed operation, for the
    /// drift check. Nothing is sent to the app.
    async fn describe(&self, _saved: &Saved<'_>, _operation: &Operation) -> Result<Value> {
        anyhow::bail!("This connector provider can't describe its actions")
    }
    /// After an unconfirmed write: did it happen? Only reads the app.
    async fn find_outcome(
        &self,
        _saved: &Saved<'_>,
        _operation: &Operation,
        _key: &str,
        _arguments: &Value,
    ) -> Result<Found> {
        Ok(Found::Unknown)
    }
    /// Subscribe the account to the provider event `trigger` (the provider's own
    /// name, from the catalog) with `config`; events go to `webhook`.
    async fn subscribe(
        &self,
        _saved: &Saved<'_>,
        _trigger: &str,
        _config: &Value,
        _webhook: &str,
    ) -> Result<Subscription> {
        anyhow::bail!("This connector provider has no events")
    }
    /// Remove a subscription at the provider. Removing one that is already
    /// gone succeeds.
    async fn unsubscribe(&self, _saved: &Saved<'_>, _subscription: &str) -> Result<()> {
        anyhow::bail!("This connector provider has no events")
    }
    /// Verify a delivery's signature and time window on the raw body with
    /// `secret` (the project's or the subscription's), then read it.
    fn parse_delivery(
        &self,
        _headers: &axum::http::HeaderMap,
        _body: &[u8],
        _secret: &str,
        _now: i64,
    ) -> Result<Vec<Delivery>> {
        anyhow::bail!("This connector provider has no events")
    }
}

/// Process-wide provider settings, set once at controller start. The providers
/// file they name is read again on every use.
pub(crate) struct Settings {
    pub mcp_servers_file: Option<PathBuf>,
    pub providers_file: Option<PathBuf>,
    /// Composio from `AYATI_COMPOSIO_KEY_FILE`, used only without the providers file.
    pub composio: Option<composio::Settings>,
    /// Composio's API: the official one, or a loopback fixture for tests.
    pub composio_base: String,
    /// Pipedream from `AYATI_PIPEDREAM_*`, used only without the providers file.
    pub pipedream: Option<pipedream::Settings>,
}
static SETTINGS: OnceLock<Settings> = OnceLock::new();

/// Registers this controller's connector settings.
pub(crate) fn configure(settings: Settings) {
    let _ = SETTINGS.set(settings);
}

/// Reads the connector settings from the controller configuration and checks
/// the providers file, so a broken one stops the controller at start.
pub(crate) fn settings(config: &crate::config::Controller) -> Result<Settings> {
    settings_from(&config.connectors, config.mcp_servers_file.clone())
}

/// The same settings from any process's connector configuration (`ayati admin`
/// uses the controller's environment).
pub(crate) fn settings_from(
    connectors: &crate::config::Connectors,
    mcp_servers_file: Option<PathBuf>,
) -> Result<Settings> {
    let composio_base = connectors
        .composio_base_url
        .clone()
        .unwrap_or_else(|| composio_api::OFFICIAL.into());
    if let Some(file) = &connectors.providers_file {
        let parsed = ProvidersFile::read(file)?;
        parsed.check_files()?;
        if let Some(entry) = parsed.composio() {
            composio::Composio::new(composio::Settings::from_entry(entry, &composio_base))
                .context("Composio in the connector providers file")?;
        }
    }
    let composio = connectors
        .composio_key_file
        .clone()
        .map(|key_file| composio::Settings {
            base: composio_base.clone(),
            key_file,
            auth_configs: connectors.composio_auth_configs.clone(),
        });
    if let Some(legacy) = &composio
        && let Err(error) = composio::Composio::new(legacy.clone())
    {
        tracing::warn!("Composio connectors are unavailable: {error}");
    }
    Ok(Settings {
        mcp_servers_file,
        pipedream: pipedream::Settings::from_config(connectors),
        providers_file: connectors.providers_file.clone(),
        composio,
        composio_base,
    })
}

/// The providers file, read now; `None` without one, or when it can't be read.
fn file() -> Option<ProvidersFile> {
    let path = SETTINGS.get()?.providers_file.as_ref()?;
    match ProvidersFile::read(path) {
        Ok(file) => Some(file),
        Err(error) => {
            tracing::warn!("The connector providers file can't be read: {error:#}");
            None
        }
    }
}

/// The private file with Composio's project webhook secret, from the providers file.
pub(crate) fn composio_webhook_secret_file() -> Option<PathBuf> {
    file()?.composio()?.webhook_secret_file.clone()
}

/// US dollars for one call through `provider`, from the providers file.
pub(crate) fn price_per_call(provider: &str) -> f64 {
    file().map_or(0.0, |f| f.price_per_call(provider))
}

/// Pipedream's project from the providers file, for its provider to register.
pub(crate) fn pipedream() -> Option<providers_file::Pipedream> {
    file()?.pipedream().cloned()
}

/// The providers this controller offers now: the operator MCP file always, and
/// Composio when its key is configured.
fn providers() -> Vec<Arc<dyn ConnectorProvider>> {
    let Some(settings) = SETTINGS.get() else {
        return vec![Arc::new(mcp_servers::McpServers::new(
            crate::identity::control::catalog::file(),
        ))];
    };
    let mut providers: Vec<Arc<dyn ConnectorProvider>> = vec![Arc::new(
        mcp_servers::McpServers::new(settings.mcp_servers_file.clone()),
    )];
    let composio = match &settings.providers_file {
        Some(_) => file().and_then(|f| {
            f.composio()
                .map(|e| composio::Settings::from_entry(e, &settings.composio_base))
        }),
        None => settings.composio.clone(),
    };
    if let Some(composio) = composio {
        match composio::Composio::new(composio) {
            Ok(provider) => providers.push(Arc::new(provider)),
            Err(error) => tracing::warn!("Composio connectors are unavailable: {error}"),
        }
    }
    let pipedream = match &settings.providers_file {
        Some(_) => file().and_then(|f| {
            f.pipedream()
                .map(|e| pipedream::Settings::from_file(e, pipedream_api::OFFICIAL))
        }),
        None => settings.pipedream.clone(),
    };
    if let Some(pipedream) = pipedream {
        match shared_pipedream(pipedream) {
            Ok(provider) => providers.push(provider),
            Err(error) => tracing::warn!("Pipedream connectors are unavailable: {error}"),
        }
    }
    providers
}

/// One Pipedream provider per settings, kept between uses so its access token
/// is reused until it expires instead of fetched again for every call.
fn shared_pipedream(settings: pipedream::Settings) -> Result<Arc<dyn ConnectorProvider>> {
    static SHARED: std::sync::Mutex<Option<(pipedream::Settings, Arc<pipedream::Pipedream>)>> =
        std::sync::Mutex::new(None);
    let mut shared = SHARED.lock().unwrap_or_else(|p| p.into_inner());
    if let Some((saved, provider)) = shared.as_ref()
        && *saved == settings
    {
        return Ok(provider.clone());
    }
    let provider = Arc::new(pipedream::Pipedream::new(settings.clone())?);
    *shared = Some((settings, provider.clone()));
    Ok(provider)
}

/// The names of the providers configured now.
pub(crate) fn configured() -> Vec<&'static str> {
    providers().iter().map(|p| p.name()).collect()
}

pub(crate) fn named(name: &str) -> Result<Arc<dyn ConnectorProvider>> {
    providers()
        .into_iter()
        .find(|p| p.name() == name)
        .context("This connector provider is not configured on this server")
}

/// What a saved connection's provider supports; nothing when it isn't configured.
pub(crate) fn capabilities_of(provider: &str) -> Capabilities {
    named(provider).map_or_else(|_| Capabilities::default(), |p| p.capabilities())
}

/// The provider's name as admins see it; agents and members never do.
pub(crate) fn label(provider: &str) -> &'static str {
    match provider {
        "composio" => "Composio",
        "pipedream" => "Pipedream",
        "mcp" => "MCP server",
        _ => "Another provider",
    }
}

/// The providers file's routes: app to providers, first choice first.
fn routes_file() -> std::collections::BTreeMap<String, Vec<String>> {
    file().map(|f| f.routes).unwrap_or_default()
}

/// This workspace's routes: its own, then the providers file's.
pub(crate) async fn routes_for(store: &PgStore, workspace: &str) -> Result<routing::Routes> {
    Ok(routing::Routes {
        workspace: store.connector_workspace_routes(workspace).await?,
        file: routes_file(),
    })
}

/// Every app offered to the workspace: one per catalog app through its routed
/// provider, plus the operator's MCP servers.
pub(crate) async fn apps(store: &PgStore, workspace: &str) -> Result<Vec<App>> {
    let routes = routes_for(store, workspace).await?;
    routing::apps_with(&providers(), workspace, &routes)
}

/// The provider that connects `app` for this workspace now.
pub(crate) async fn provider_for(
    store: &PgStore,
    workspace: &str,
    app: &str,
) -> Result<Arc<dyn ConnectorProvider>> {
    let routes = routes_for(store, workspace).await?;
    routing::provider_with(&providers(), workspace, app, &routes)
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

/// A Pipedream provider against a local fake, for tests outside this module.
#[cfg(test)]
pub(crate) fn pipedream_for_tests(
    fake: &fake_pipedream::FakePipedream,
) -> Arc<dyn ConnectorProvider> {
    Arc::new(pipedream::Pipedream::new(fake.settings()).unwrap())
}

/// A fingerprint of everything that defines a saved account.
pub(crate) fn fingerprint(provider: &str, definition: &Value, binding: Option<&Value>) -> String {
    crate::identity::provider::digest(&format!(
        "{provider}\n{definition}\n{}",
        binding.cloned().unwrap_or(Value::Null)
    ))
}

//! The registry of connector providers: which are configured on this server,
//! built from the controller's settings and the providers file (read again on
//! every use), and which one serves an app for a workspace.
use super::*;
use std::sync::OnceLock;

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
pub(super) fn providers() -> Vec<Arc<dyn ConnectorProvider>> {
    let Some(settings) = SETTINGS.get() else {
        return vec![
            Arc::new(mcp_servers::McpServers::new(
                crate::identity::control::catalog::file(),
            )),
            Arc::new(systems::Systems),
        ];
    };
    let mut providers: Vec<Arc<dyn ConnectorProvider>> = vec![
        Arc::new(mcp_servers::McpServers::new(
            settings.mcp_servers_file.clone(),
        )),
        Arc::new(systems::Systems),
    ];
    if let Some(composio) = composio_from(settings) {
        match composio::Composio::new(composio) {
            Ok(provider) => providers.push(Arc::new(provider)),
            Err(error) => tracing::warn!("Composio connectors are unavailable: {error}"),
        }
    }
    let pipedream = match &settings.providers_file {
        Some(_) => file().and_then(|f| {
            let entry = f.pipedream()?;
            pipedream::Settings::from_file(entry, pipedream_api::OFFICIAL)
                .inspect_err(|e| tracing::warn!("Pipedream in the providers file: {e:#}"))
                .ok()
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

/// Composio's settings now: from the providers file when there is one (read
/// again here), else from `AYATI_COMPOSIO_*`.
pub(crate) fn composio_from(settings: &Settings) -> Option<composio::Settings> {
    match &settings.providers_file {
        Some(path) => match ProvidersFile::read(path) {
            Ok(file) => file
                .composio()
                .map(|e| composio::Settings::from_entry(e, &settings.composio_base)),
            Err(error) => {
                tracing::warn!("The connector providers file can't be read: {error:#}");
                None
            }
        },
        None => settings.composio.clone(),
    }
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
        "system" => "Connected system",
        _ => "Another provider",
    }
}

/// The providers file's routes: app to providers, first choice first.
pub(super) fn routes_file() -> std::collections::BTreeMap<String, Vec<String>> {
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

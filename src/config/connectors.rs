//! Connector provider settings (runtime/connector-providers.md). The providers
//! file is read again on every use; its contents are checked at controller start.
use super::{Reader, absolute};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Clone, Default)]
pub struct Connectors {
    /// `AYATI_CONNECTOR_PROVIDERS_FILE`: providers, key files, routes and prices.
    pub providers_file: Option<PathBuf>,
    /// Composio's project API key (a private 0600 file), used only without the
    /// providers file; no file, no Composio.
    pub composio_key_file: Option<PathBuf>,
    /// A loopback fixture for tests; the official API otherwise.
    pub composio_base_url: Option<String>,
    /// Toolkit to Composio auth config, for apps that sign in with your own OAuth app.
    pub composio_auth_configs: BTreeMap<String, String>,
    /// Pipedream from `AYATI_PIPEDREAM_*`, used only without the providers file.
    pub pipedream: Option<super::Pipedream>,
}

impl Connectors {
    pub(crate) fn read(env: &mut Reader) -> Self {
        let connectors = Self {
            providers_file: absolute(env, "AYATI_CONNECTOR_PROVIDERS_FILE"),
            composio_key_file: absolute(env, "AYATI_COMPOSIO_KEY_FILE"),
            composio_base_url: env.text("AYATI_COMPOSIO_BASE_URL"),
            composio_auth_configs: composio_auth_configs(env),
            pipedream: super::Pipedream::read(env),
        };
        if connectors.providers_file.is_some()
            && (connectors.composio_key_file.is_some()
                || !connectors.composio_auth_configs.is_empty()
                || connectors.pipedream.is_some())
        {
            env.problem(
                "Set Composio and Pipedream in AYATI_CONNECTOR_PROVIDERS_FILE, without AYATI_COMPOSIO_* or AYATI_PIPEDREAM_* settings",
            );
        }
        connectors
    }
}

/// `gmail=ac_1,googlecalendar=ac_2`: which Composio auth config each toolkit uses.
fn composio_auth_configs(env: &mut Reader) -> BTreeMap<String, String> {
    let mut configs = BTreeMap::new();
    for pair in env
        .text("AYATI_COMPOSIO_AUTH_CONFIGS")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        match pair.split_once('=') {
            Some((toolkit, config)) if !toolkit.trim().is_empty() && !config.trim().is_empty() => {
                configs.insert(toolkit.trim().to_lowercase(), config.trim().to_owned());
            }
            _ => env.problem(
                "AYATI_COMPOSIO_AUTH_CONFIGS must look like gmail=ac_123,googlecalendar=ac_456",
            ),
        }
    }
    configs
}

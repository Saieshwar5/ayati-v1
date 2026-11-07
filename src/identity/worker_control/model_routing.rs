//! What Cuckoo Control adds to the model gateway (schema 91). The catalog's
//! router (`identity::models::router`) still orders routes and rests failing
//! providers; before a call this module:
//!
//! - refuses a suspended workspace, or any call once the platform has reached
//!   its daily ceiling (`platform.daily_ceiling_usd`, on provider cost);
//! - drops routes whose provider an operator switched off;
//! - finds each provider's key: a provider added in the console (sealed with
//!   `AYATI_SECRETS_KEY_FILE`), else the deployment's providers file.
use super::super::postgres::{
    PgStore,
    operator::{providers::FileProvider, spend},
};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// The ceiling is checked against totals at most this old, so a busy platform
/// does not sum the day's cost on every call.
const LIMIT_CACHE: Duration = Duration::from_secs(15);

/// Per workspace: when the limits were checked, and the refusal if any.
type LimitCache = HashMap<String, (Instant, Option<String>)>;
static LIMITS: Mutex<Option<LimitCache>> = Mutex::new(None);

/// Why Cuckoo will not send this workspace's next model call, if it won't.
pub(crate) async fn refusal(store: &PgStore, workspace: &str) -> Result<Option<String>> {
    if let Some((at, refusal)) = LIMITS
        .lock()
        .unwrap()
        .get_or_insert_default()
        .get(workspace)
        && at.elapsed() < LIMIT_CACHE
    {
        return Ok(refusal.clone());
    }
    let refusal = spend::refusal(store, workspace).await?;
    let mut cache = LIMITS.lock().unwrap();
    let cache = cache.get_or_insert_default();
    if cache.len() > 10_000 {
        cache.clear();
    }
    cache.insert(workspace.to_owned(), (Instant::now(), refusal.clone()));
    Ok(refusal)
}

/// Tests change limits and check them again at once.
#[cfg(test)]
pub(crate) fn forget_limits() {
    LIMITS.lock().unwrap().take();
}

/// The routes whose provider is on, in their order, and the operator's note for
/// each switched-off provider that was skipped.
pub(crate) async fn switched_on<T>(
    store: &PgStore,
    routes: Vec<(String, T)>,
) -> Result<(Vec<(String, T)>, Vec<String>)> {
    let (mut on, mut skipped) = (Vec::new(), Vec::new());
    for (provider, route) in routes {
        match store.model_provider_off(&provider).await? {
            Some(note) => {
                let note = format!("Cuckoo paused {provider}: {note}");
                if !skipped.contains(&note) {
                    skipped.push(note);
                }
            }
            None => on.push((provider, route)),
        }
    }
    Ok((on, skipped))
}

/// A provider's address and key. A key kept in Cuckoo Control wins over the
/// providers file's key file; the address comes from the console provider, or
/// from the providers file.
pub(crate) async fn credentials(
    app: &super::super::App,
    provider: &str,
) -> Result<(String, String)> {
    let file = app.config.model_providers_file.as_deref();
    if let Some(stored) = app.store.model_provider_stored(provider).await? {
        let sealer = app
            .config
            .secrets_key_file
            .as_deref()
            .map(super::super::secrets::Sealer::new)
            .context("AYATI_SECRETS_KEY_FILE is needed for keys kept in Cuckoo Control")?;
        let base = match stored.base_url {
            Some(base) => base,
            None => {
                let file =
                    file.with_context(|| format!("No model provider {provider} is configured"))?;
                super::model_gateway::provider_entry(file, provider)?.0
            }
        };
        return Ok((base, sealer.open(provider, &stored.sealed)?));
    }
    let file = file.with_context(|| format!("No model provider {provider} is configured"))?;
    super::model_gateway::provider(file, provider)
}

/// Providers from the deployment's file, without reading any key.
pub(crate) fn file_providers(file: Option<&std::path::Path>) -> Vec<FileProvider> {
    #[derive(serde::Deserialize)]
    struct Entry {
        provider: String,
        base_url: String,
    }
    let Some(file) = file else { return vec![] };
    std::fs::read(file)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Vec<Entry>>(&bytes).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|e| FileProvider {
            name: e.provider,
            base_url: e.base_url,
        })
        .collect()
}

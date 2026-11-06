//! Which provider and model serve a model request (schema 87, Cuckoo Control).
//!
//! Before any call: a suspended workspace, a workspace over its monthly cap or a
//! platform over its daily ceiling is refused. Then the requested model's route
//! (if an operator set one) gives the main target and an optional backup, in an
//! order chosen by the split; without a route the assignment's own provider and
//! model are used, as before. Targets whose provider is switched off, or whose
//! model the catalog blocks, are skipped.
use super::super::postgres::{
    PgStore,
    operator::{models::Route, providers::FileProvider, spend},
};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Spend limits are checked against totals at most this old, so one busy
/// workspace does not sum its month on every call.
const LIMIT_CACHE: Duration = Duration::from_secs(15);

pub(crate) struct Target {
    pub provider: String,
    pub model: String,
    pub base_url: String,
    pub key: String,
}

pub(crate) struct Plan {
    pub targets: Vec<Target>,
    /// How long to wait for a target to start answering before the next one.
    pub timeout: Duration,
    pub routed: bool,
}

/// A request Cuckoo will not send; the message is shown to the agent's user.
pub(crate) struct Refused(pub String);

/// Per workspace: when the limits were checked, and the refusal if any.
type LimitCache = HashMap<String, (Instant, Option<String>)>;
static LIMITS: Mutex<Option<LimitCache>> = Mutex::new(None);

async fn limit_refusal(store: &PgStore, workspace: &str) -> Result<Option<String>> {
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

/// Splits by request: every call in one request goes the same way, so one
/// answer does not change model halfway.
pub(super) fn main_first(sticky: &str, main_percent: i32) -> bool {
    // FNV-1a: stable across processes and releases, unlike the std hasher.
    let hash = sticky.bytes().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ b as u64).wrapping_mul(0x100000001b3)
    });
    (hash % 100) < main_percent as u64
}

/// The (provider, model) pairs to try, in order, before switches and the catalog.
pub(super) fn order(
    route: Option<&Route>,
    provider: &str,
    model: &str,
    sticky: &str,
) -> Vec<(String, String)> {
    match route {
        None => vec![(provider.to_owned(), model.to_owned())],
        Some(route) => {
            let main = (route.main.provider.clone(), route.main.model.clone());
            match &route.second {
                None => vec![main],
                Some(second) => {
                    let second = (second.provider.clone(), second.model.clone());
                    if main_first(sticky, route.main_percent) {
                        vec![main, second]
                    } else {
                        vec![second, main]
                    }
                }
            }
        }
    }
}

pub(crate) async fn plan(
    app: &super::super::App,
    workspace: &str,
    provider: &str,
    model: &str,
    sticky: &str,
) -> Result<std::result::Result<Plan, Refused>> {
    let store = &app.store;
    if let Some(refusal) = limit_refusal(store, workspace).await? {
        return Ok(Err(Refused(refusal)));
    }
    let route = store.model_route(model).await?;
    let mut targets = Vec::new();
    let mut skipped = Vec::new();
    for (provider, model) in order(route.as_ref(), provider, model, sticky) {
        if let Some(note) = store.model_provider_off(&provider).await? {
            skipped.push(format!("Cuckoo paused {provider}: {note}"));
            continue;
        }
        if store.model_blocked(&provider, &model).await? {
            skipped.push(format!("{model} is not available on Cuckoo"));
            continue;
        }
        match credentials(app, &provider).await {
            Ok((base_url, key)) => targets.push(Target {
                provider,
                model,
                base_url,
                key,
            }),
            Err(error) => {
                // The provider name only: credential errors never carry key text.
                tracing::warn!(
                    provider,
                    error = format!("{error:#}"),
                    "Model provider unavailable"
                );
                skipped.push(format!("{provider} is not set up"));
            }
        }
    }
    if targets.is_empty() {
        let reason = skipped.join("; ");
        return Ok(Err(Refused(if reason.is_empty() {
            "No model provider is available".into()
        } else {
            reason
        })));
    }
    Ok(Ok(Plan {
        timeout: Duration::from_secs(
            route
                .as_ref()
                .map(|r| r.timeout_seconds as u64)
                .unwrap_or(600),
        ),
        routed: route.is_some(),
        targets,
    }))
}

/// A console provider's sealed key, else the deployment's providers file.
async fn credentials(app: &super::super::App, provider: &str) -> Result<(String, String)> {
    if let Some(stored) = app.store.model_provider_stored(provider).await? {
        let sealer = app
            .config
            .secrets_key_file
            .as_deref()
            .map(super::super::secrets::Sealer::new)
            .context("AYATI_SECRETS_KEY_FILE is needed to use providers added in Cuckoo Control")?;
        return Ok((stored.base_url, sealer.open(provider, &stored.sealed)?));
    }
    let file = app
        .config
        .model_providers_file
        .as_deref()
        .context("The model gateway has no providers file")?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::postgres::operator::models::Target as RouteTarget;

    fn route(main_percent: i32) -> Route {
        Route {
            model: "kimi-k2".into(),
            main: RouteTarget {
                provider: "fireworks".into(),
                model: "accounts/fireworks/models/kimi-k2".into(),
            },
            second: Some(RouteTarget {
                provider: "openai".into(),
                model: "gpt-5-mini".into(),
            }),
            main_percent,
            timeout_seconds: 30,
        }
    }

    #[test]
    fn without_a_route_the_assignment_model_is_used() {
        assert_eq!(
            order(None, "openai", "m1", "job"),
            vec![("openai".into(), "m1".into())]
        );
    }

    #[test]
    fn the_split_is_stable_per_request_and_close_to_the_percentage() {
        let r = route(90);
        assert_eq!(
            order(Some(&r), "x", "kimi-k2", "job-1"),
            order(Some(&r), "x", "kimi-k2", "job-1")
        );
        let main = (0..10_000)
            .filter(|i| main_first(&format!("job-{i}"), 90))
            .count();
        assert!((8_700..=9_300).contains(&main), "{main}");
        // Whichever goes first, the other is the backup.
        for i in 0..50 {
            let o = order(Some(&r), "x", "kimi-k2", &format!("job-{i}"));
            assert_eq!(o.len(), 2);
            assert_ne!(o[0], o[1]);
        }
        assert!(
            (0..100)
                .all(|i| order(Some(&route(100)), "x", "k", &i.to_string())[0].0 == "fireworks")
        );
        assert!(
            (0..100).all(|i| order(Some(&route(0)), "x", "k", &i.to_string())[0].0 == "openai")
        );
    }
}

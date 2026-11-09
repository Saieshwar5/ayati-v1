//! Provider subscriptions end to end (schema 109): creating one for a trigger
//! from an app and stopping unused ones at their provider. Providers are always
//! called outside a database transaction; nothing is saved half-made.
use super::model::FromApp;
use crate::identity::connectors::{ConnectorProvider, Refusal, Saved};
use crate::identity::postgres::{PgStore, event_subscriptions::app_triggers::Created};
use crate::identity::secrets::Sealer;
use anyhow::{Context as _, Result};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

/// How long a provider may take to subscribe or unsubscribe.
const PROVIDER_WAIT: Duration = Duration::from_secs(30);
pub(crate) const NOT_SET_UP: &str =
    "App events aren't set up on this server yet. A Cuckoo administrator needs to set them up.";
pub(crate) const NO_ADDRESS: &str = "App events need this server's public web address, which isn't set yet. A Cuckoo administrator needs to set it.";

/// Finds the provider serving a connection, by its saved provider name.
pub(crate) type Providers<'a> = &'a (dyn Fn(&str) -> Result<Arc<dyn ConnectorProvider>> + Sync);

/// What creating a subscription needs besides the request.
pub(crate) struct Context<'a> {
    pub store: &'a PgStore,
    /// `AYATI_PUBLIC_ORIGIN`: where providers deliver.
    pub origin: &'a str,
    pub sealer: Option<Sealer>,
    pub providers: Providers<'a>,
    /// Why a provider's events can't reach this server; absent when they can.
    pub unavailable: BTreeMap<String, String>,
}

/// Why `provider`'s events can't reach this server, or `None` when they can:
/// a public HTTPS address, the secrets key for sealed signing keys, a provider
/// with events, and Composio's project webhook registered.
pub(crate) async fn unavailable(
    store: &PgStore,
    config: &crate::config::Controller,
    providers: Providers<'_>,
    provider: &str,
) -> Option<String> {
    if !config.hosted() {
        return Some(NO_ADDRESS.into());
    }
    let offered = providers(provider).is_ok_and(|p| p.capabilities().triggers);
    if !offered || config.secrets_key_file.is_none() {
        return Some(NOT_SET_UP.into());
    }
    if provider == "composio" && !matches!(store.connector_event_setup(provider).await, Ok(Some(_)))
    {
        return Some(NOT_SET_UP.into());
    }
    None
}

/// Where a provider delivers a subscription's events: Composio has one URL per
/// project; other providers get one URL per subscription.
pub(crate) fn hook_url(origin: &str, provider: &str, subscription: &str) -> String {
    if provider == "composio" {
        format!("{origin}/hooks/composio")
    } else {
        format!("{origin}/hooks/{provider}/{subscription}")
    }
}

/// The seal label of a subscription's signing key: bound to its row, so a
/// sealed key moved to another subscription does not open.
pub(crate) fn key_label(w: &str, id: &str) -> String {
    format!("connector-subscription/{w}/{id}")
}

/// `POST …/event-triggers` with `{connection, event, …}`: checks authority,
/// shares the account's active subscription of that event or subscribes at the
/// provider, then saves subscription and trigger together. An exact repeat
/// returns the saved trigger without calling the provider.
pub(crate) async fn create_from_app(
    cx: &Context<'_>,
    user: &crate::identity::User,
    w: &str,
    agent: &str,
    input: &FromApp,
) -> Result<Value> {
    let plan = match cx.store.app_trigger_plan(user, w, agent, input).await? {
        Ok(saved) => return Ok(saved),
        Err(plan) => plan,
    };
    if let Some(why) = cx.unavailable.get(&plan.provider) {
        anyhow::bail!("{why}");
    }
    if plan.existing.is_some() {
        return Ok(cx
            .store
            .save_app_trigger(user, w, agent, input, None)
            .await?
            .0);
    }
    let provider = (cx.providers)(&plan.provider)?;
    anyhow::ensure!(provider.capabilities().triggers, NOT_SET_UP);
    let id = uuid::Uuid::new_v4().to_string();
    let saved = Saved {
        workspace: w,
        server: &plan.server,
        binding: Some(&plan.binding),
        fingerprint: &plan.fingerprint,
    };
    let webhook = hook_url(cx.origin, &plan.provider, &id);
    let made = match tokio::time::timeout(
        PROVIDER_WAIT,
        provider.subscribe(&saved, &plan.event.trigger, &plan.event.config, &webhook),
    )
    .await
    {
        Err(_) => anyhow::bail!(
            "The app's provider did not answer in time. Nothing was saved; try again."
        ),
        Ok(Err(error)) => {
            if matches!(
                error.downcast_ref::<Refusal>(),
                Some(Refusal::NeedsReconnect)
            ) {
                cx.store
                    .connector_needs_reconnect(w, &input.connection)
                    .await?;
            }
            return Err(error);
        }
        Ok(Ok(made)) => made,
    };
    let saving = async {
        let signing_key = match &made.signing_key {
            None => None,
            Some(key) => Some(
                cx.sealer
                    .as_ref()
                    .context("Set AYATI_SECRETS_KEY_FILE to keep the provider's signing keys")?
                    .seal(&key_label(w, &id), key)?,
            ),
        };
        let created = Created {
            id: id.clone(),
            provider_id: made.id.clone(),
            signing_key,
        };
        cx.store
            .save_app_trigger(user, w, agent, input, Some(&created))
            .await
    };
    match saving.await {
        Ok((data, unused)) => {
            if let Some(unused) = unused {
                stop(cx.store, provider.as_ref(), &saved, &unused).await;
            }
            Ok(data)
        }
        Err(error) => {
            stop(cx.store, provider.as_ref(), &saved, &made.id).await;
            Err(error)
        }
    }
}

/// Whether an active saved subscription uses this provider id.
async fn in_use(store: &PgStore, provider: &str, id: &str) -> bool {
    matches!(store.subscription_by_provider(provider, id).await,
        Ok(Some(found)) if found.source.available().is_ok())
}

/// Removes a provider subscription nothing saved uses. One an active
/// subscription shares (the provider reused its id) is left alone.
async fn stop(store: &PgStore, provider: &dyn ConnectorProvider, saved: &Saved<'_>, id: &str) {
    if in_use(store, provider.name(), id).await {
        return;
    }
    let done = tokio::time::timeout(PROVIDER_WAIT, provider.unsubscribe(saved, id))
        .await
        .is_ok_and(|r| r.is_ok());
    if !done {
        tracing::warn!(
            "A {} subscription Cuckoo didn't save may still be active",
            provider.name()
        );
    }
}

/// Stops removed and broken subscriptions at their provider (a removed last
/// trigger, a disconnected or reconnected account), retrying failures with
/// backoff. Runs in the shared admission loop and right after a removal.
/// A provider that reuses an id for a new subscription in the moment between
/// the check and the call could still lose it; Composio issues new ids.
pub(crate) async fn release(store: &PgStore, w: Option<&str>, providers: Providers<'_>) -> usize {
    let Ok(list) = store.subscriptions_to_release(w, 20).await else {
        return 0;
    };
    let mut done = 0;
    for r in list {
        let stopped = in_use(store, &r.provider, &r.provider_id).await
            || match providers(&r.provider) {
                Ok(provider) => {
                    let saved = Saved {
                        workspace: &r.workspace,
                        server: &r.server,
                        binding: Some(&r.binding),
                        fingerprint: "",
                    };
                    tokio::time::timeout(
                        PROVIDER_WAIT,
                        provider.unsubscribe(&saved, &r.provider_id),
                    )
                    .await
                    .is_ok_and(|r| r.is_ok())
                }
                Err(_) => false,
            };
        if !stopped {
            tracing::warn!(
                "The {} provider did not confirm stopping an app event; retrying later",
                r.provider
            );
        }
        if store
            .subscription_release_tried(&r.workspace, &r.id, stopped)
            .await
            .is_ok()
            && stopped
        {
            done += 1;
        }
    }
    done
}

/// The admission loop's share: stop unused subscriptions at their provider.
/// It never reports work, so a failing provider can't make the loop spin.
pub(crate) async fn sweep(store: &PgStore) -> Result<usize> {
    release(store, None, &crate::identity::connectors::named).await;
    Ok(0)
}

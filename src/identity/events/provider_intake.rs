//! Signed provider deliveries into the existing receipts (schema 109). The
//! signature and ±300 s window are checked on the raw body first; then each
//! event is routed to its saved subscription and anything that doesn't match
//! is refused. The receipt is saved before the provider hears 200.
use super::model::InboxFull;
use super::subscriptions::key_label;
use crate::identity::connectors::{ConnectorProvider, Delivery, DeliveryKind, composio_triggers};
use crate::identity::postgres::{PgStore, event_subscriptions::Hooked};
use crate::identity::secrets::Sealer;
use crate::identity::{ApiResult, Error};
use axum::http::{HeaderMap, StatusCode};
use serde_json::{Value, json};

/// The seal label of Composio's project signing secret.
pub(crate) const COMPOSIO_SECRET: &str = "connector-events/composio";

fn refused(status: StatusCode, message: &str) -> Error {
    Error(anyhow::anyhow!("{message}"), status)
}
pub(crate) fn unavailable() -> Error {
    refused(
        StatusCode::SERVICE_UNAVAILABLE,
        "Event intake is unavailable. Check operator setup and retry.",
    )
}
fn forged() -> Error {
    refused(StatusCode::UNAUTHORIZED, "Event identity does not match")
}
fn unknown() -> Error {
    refused(StatusCode::NOT_FOUND, "Unknown event subscription")
}

/// `POST /hooks/composio`: one URL and secret for the whole project. An event
/// is routed by its Composio user (`cuckoo-<workspace>`) and trigger id to the
/// saved subscription; account expiry marks the bound connections.
pub(crate) async fn composio(
    store: &PgStore,
    provider: &dyn ConnectorProvider,
    headers: &HeaderMap,
    body: &[u8],
    secret: &str,
    limit: i64,
) -> ApiResult<Value> {
    let deliveries = provider
        .parse_delivery(headers, body, secret, crate::identity::user::now())
        .map_err(|_| refused(StatusCode::UNAUTHORIZED, "Webhook authentication failed"))?;
    let mut answer = json!({"accepted":true});
    for delivery in deliveries {
        answer = match &delivery.kind {
            DeliveryKind::Ignored => json!({"accepted":true,"ignored":true}),
            DeliveryKind::AccountExpired => {
                let account = delivery.account.as_deref().ok_or_else(forged)?;
                let marked = store
                    .connector_account_expired(provider.name(), account, delivery.user.as_deref())
                    .await
                    .map_err(|_| unavailable())?;
                json!({"accepted":true,"reconnect":marked.len()})
            }
            DeliveryKind::Event { subscription, .. } => {
                let w = delivery
                    .user
                    .as_deref()
                    .and_then(composio_triggers::workspace_of)
                    .ok_or_else(forged)?;
                let hooked = store
                    .subscription_by_provider(provider.name(), subscription)
                    .await
                    .map_err(|_| unavailable())?
                    .ok_or_else(unknown)?;
                if hooked.source.workspace != w {
                    return Err(forged());
                }
                accept(store, &hooked, &delivery, limit).await?
            }
        };
    }
    Ok(answer)
}

/// `POST /hooks/{provider}/{subscription}`: a per-subscription URL whose
/// signing key, sealed with that subscription, verifies the delivery.
pub(crate) async fn signed(
    store: &PgStore,
    provider: &dyn ConnectorProvider,
    sealer: Option<&Sealer>,
    subscription: &str,
    headers: &HeaderMap,
    body: &[u8],
    limit: i64,
) -> ApiResult<Value> {
    uuid::Uuid::parse_str(subscription).map_err(|_| unknown())?;
    let hooked = store
        .subscription_by_id(provider.name(), subscription)
        .await
        .map_err(|_| unavailable())?
        .ok_or_else(unknown)?;
    let source = &hooked.source;
    let sealed = hooked.signing_key.as_ref().ok_or_else(unavailable)?;
    let key = sealer
        .ok_or_else(unavailable)?
        .open(&key_label(&source.workspace, &source.id), sealed)
        .map_err(|_| unavailable())?;
    let deliveries = provider
        .parse_delivery(headers, body, &key, crate::identity::user::now())
        .map_err(|_| refused(StatusCode::UNAUTHORIZED, "Webhook authentication failed"))?;
    let mut answer = json!({"accepted":true});
    for mut delivery in deliveries {
        let named_other = delivery.account.as_ref().is_some_and(|a| {
            !source.connected_account_id.is_empty() && *a != source.connected_account_id
        });
        answer = match &mut delivery.kind {
            DeliveryKind::Ignored => json!({"accepted":true,"ignored":true}),
            DeliveryKind::AccountExpired if named_other => return Err(forged()),
            DeliveryKind::AccountExpired => {
                store
                    .connector_needs_reconnect(&source.workspace, &source.connection)
                    .await
                    .map_err(|_| unavailable())?;
                json!({"accepted":true,"reconnect":1})
            }
            DeliveryKind::Event {
                subscription,
                trigger,
                ..
            } => {
                // The URL and its key name the subscription; a delivery that
                // also names one must name the same.
                if subscription.is_empty() {
                    *subscription = source.trigger_id.clone();
                }
                if trigger.is_empty() {
                    *trigger = source.trigger_slug.clone();
                }
                accept(store, &hooked, &delivery, limit).await?
            }
        };
    }
    Ok(answer)
}

/// Saves the receipt (and any matched deliveries) before answering, exactly
/// as for operator sources: a retry is a duplicate, changed content a conflict.
async fn accept(
    store: &PgStore,
    hooked: &Hooked,
    delivery: &Delivery,
    limit: i64,
) -> ApiResult<Value> {
    let source = &hooked.source;
    if source.available().is_err() {
        return Err(refused(
            StatusCode::NOT_FOUND,
            "This event subscription is no longer active",
        ));
    }
    let event = super::provider_events::normalized(delivery).ok_or_else(unavailable)?;
    source.project(&delivery.event_id, &event).map_err(|_| {
        refused(
            StatusCode::UNAUTHORIZED,
            "Event identity or reviewed fields do not match",
        )
    })?;
    match store
        .accept_event(source, &delivery.event_id, &event, limit)
        .await
    {
        Ok(data) => Ok(data),
        Err(e) if e.is::<InboxFull>() => Err(Error(e, StatusCode::TOO_MANY_REQUESTS)),
        Err(e) if e.to_string() == "Event ID belongs to different content" => {
            Err(Error(e, StatusCode::CONFLICT))
        }
        Err(_) => Err(unavailable()),
    }
}

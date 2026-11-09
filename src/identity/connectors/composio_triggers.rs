//! Composio events (schema 109): a trigger instance per account and event, the
//! project's one webhook subscription, and reading its signed deliveries.
//! Composio signs with Standard Webhooks headers over `{id}.{timestamp}.{body}`.
use super::{
    Delivery, DeliveryKind, Subscription,
    composio_api::{self, ComposioApi, Reply},
};
use anyhow::{Context, Result, ensure};
use reqwest::Method;
use serde_json::{Value, json};

/// The events Cuckoo asks Composio to deliver to its project webhook.
pub(crate) const EVENTS: [&str; 2] = [TRIGGER_MESSAGE, ACCOUNT_EXPIRED];
const TRIGGER_MESSAGE: &str = "composio.trigger.message";
const ACCOUNT_EXPIRED: &str = "composio.connected_account.expired";

/// Creates (or, for the same account and trigger, updates) a trigger instance.
pub(super) async fn subscribe(
    api: &ComposioApi,
    account: &str,
    user: &str,
    trigger: &str,
    config: &Value,
) -> Result<Subscription> {
    let reply = api
        .request(
            Method::POST,
            &format!("trigger_instances/{}/upsert", composio_api::id(trigger)?),
            Some(&json!({"user_id":user,"connected_account_id":account,"trigger_config":config})),
        )
        .await?;
    let id = reply["trigger_id"]
        .as_str()
        .context("Composio omitted the trigger")?;
    Ok(Subscription {
        id: composio_api::id(id)?.to_owned(),
        signing_key: None,
    })
}

/// Deletes a trigger instance; one that is already gone counts as removed.
pub(super) async fn unsubscribe(api: &ComposioApi, id: &str) -> Result<()> {
    let path = format!("trigger_instances/manage/{}", composio_api::id(id)?);
    match api.send(Method::DELETE, &path, None).await? {
        Reply::Ok(_) | Reply::Status(404) => Ok(()),
        Reply::Status(code) => anyhow::bail!("Composio did not remove the trigger (HTTP {code})"),
    }
}

/// The project's webhook subscription, as registered.
pub(crate) struct ProjectWebhook {
    pub id: String,
    /// Composio's signing secret. The caller seals it; never logged or shown.
    pub secret: String,
}

/// Points the project's one webhook subscription at `url` for [`EVENTS`],
/// creating it or, when the project has one already, updating it.
pub(crate) async fn register_webhook(
    config: &crate::config::Controller,
    url: &str,
) -> Result<ProjectWebhook> {
    let settings = super::composio_from(&super::settings_from(&config.connectors, None)?)
        .context("Composio is not configured on this server")?;
    let api = ComposioApi::new(&settings.base, settings.key_file)?;
    let body = json!({"webhook_url":url,"enabled_events":EVENTS,"version":"V3"});
    let saved = match api
        .send(Method::POST, "webhook_subscriptions", Some(&body))
        .await?
    {
        Reply::Ok(saved) => saved,
        Reply::Status(409) => {
            let list = api
                .request(Method::GET, "webhook_subscriptions?limit=50", None)
                .await?;
            let id = list["items"][0]["id"]
                .as_str()
                .context("Composio omitted the project's webhook subscription")?;
            let path = format!("webhook_subscriptions/{}", composio_api::id(id)?);
            api.request(Method::PATCH, &path, Some(&body)).await?
        }
        Reply::Status(code) => {
            anyhow::bail!("Composio refused the webhook subscription (HTTP {code})")
        }
    };
    let id = composio_api::id(
        saved["id"]
            .as_str()
            .context("Composio omitted the subscription")?,
    )?;
    let secret = saved["secret"]
        .as_str()
        .context("Composio omitted the signing secret")?;
    ensure!(
        (16..=4096).contains(&secret.len()) && secret.bytes().all(|b| b.is_ascii_graphic()),
        "Composio returned an unexpected signing secret"
    );
    Ok(ProjectWebhook {
        id: id.to_owned(),
        secret: secret.to_owned(),
    })
}

/// The workspace a Composio user belongs to: each workspace is one Composio
/// user, `cuckoo-<workspace id>`.
pub(crate) fn workspace_of(user: &str) -> Option<String> {
    let id = user.strip_prefix("cuckoo-")?;
    uuid::Uuid::parse_str(id).ok()?;
    Some(id.to_owned())
}

/// Verifies a delivery's signature and ±300 s window on the raw body, then
/// reads its V3 envelope. Nothing is parsed before the signature passes.
pub(super) fn parse(
    headers: &axum::http::HeaderMap,
    body: &[u8],
    secret: &str,
    now: i64,
) -> Result<Vec<Delivery>> {
    let header = |name: &str| -> Result<&str> {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .context("Webhook authentication required")
    };
    let id = header("webhook-id")?;
    crate::identity::events::webhook::verify(
        secret,
        id,
        header("webhook-timestamp")?,
        header("webhook-signature")?,
        body,
        now,
    )?;
    let event: Value = serde_json::from_slice(body).context("Invalid event envelope")?;
    let text = |value: &Value| -> Result<String> {
        Ok(composio_api::id(
            value
                .as_str()
                .context("Composio omitted an event identity")?,
        )?
        .to_owned())
    };
    let metadata = &event["metadata"];
    let delivery = match event["type"].as_str() {
        Some(TRIGGER_MESSAGE) => {
            ensure!(
                event["id"] == id,
                "The event id does not match its signature"
            );
            ensure!(event["data"].is_object(), "Event data must be an object");
            Delivery {
                event_id: id.to_owned(),
                user: Some(text(&metadata["user_id"])?),
                account: Some(text(&metadata["connected_account_id"])?),
                kind: DeliveryKind::Event {
                    subscription: text(&metadata["trigger_id"])?,
                    trigger: text(&metadata["trigger_slug"])?,
                    data: event["data"].clone(),
                },
            }
        }
        // The expiry event names the account in `data.id` and no user.
        Some(ACCOUNT_EXPIRED) => Delivery {
            event_id: id.to_owned(),
            user: metadata["user_id"].as_str().map(str::to_owned),
            account: Some(text(&event["data"]["id"])?),
            kind: DeliveryKind::AccountExpired,
        },
        _ => Delivery {
            event_id: id.to_owned(),
            user: None,
            account: None,
            kind: DeliveryKind::Ignored,
        },
    };
    Ok(vec![delivery])
}

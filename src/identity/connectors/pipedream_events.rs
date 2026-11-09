//! Pipedream triggers: deploying one for an account, removing it, and reading
//! its signed deliveries.
//!
//! Each deployed trigger posts to its own `webhook_url` and signs every delivery
//! with its own key: `x-pd-signature: t=<unix seconds>,v1=<hex HMAC-SHA256 of
//! "{t}.{raw body}">`. The intake route (`POST /hooks/pipedream/{subscription}`)
//! looks up the subscription its URL names and passes that subscription's key
//! here, so a valid signature already proves which subscription a delivery
//! belongs to. The delivery's `subscription` is Pipedream's `context.emitter_id`
//! (the `dc_…` id) when the delivery carries one; the intake must refuse a
//! delivery whose non-empty `subscription` differs from the URL's. It is empty
//! when Pipedream doesn't name the emitter.
//!
//! Unconfirmed until a live run: Pipedream documents the signature but not the
//! delivery body. Cuckoo reads `{"event": …, "context": {"id", "emitter_id",
//! "external_user_id"}}` (the shape Pipedream gives workflow events), and a body
//! without `event` as the event itself. The event id is `context.id`, or the
//! SHA-256 of the raw body when Pipedream sends none.
use super::pipedream::Pipedream;
use super::pipedream_accounts::{Binding, user};
use super::pipedream_api::{Reply, id};
use super::{Delivery, DeliveryKind, Refusal, Saved, Subscription};
use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use reqwest::Method;
use ring::hmac;
use serde_json::{Map, Value, json};

/// Seconds a signature stays acceptable, either side of its time.
const WINDOW: i64 = 300;
/// Larger deliveries are refused before anything is read.
const MAX_BODY: usize = 256 * 1024;

impl Pipedream {
    /// Deploys the trigger component `trigger` for the saved account, at its
    /// current version, with `config` (component prop names to plain values).
    /// Old events are not replayed (`emit_on_deploy: false`).
    pub(super) async fn deploy(
        &self,
        saved: &Saved<'_>,
        trigger: &str,
        config: &Value,
        webhook: &str,
    ) -> Result<Subscription> {
        let binding = Binding::parse(saved.binding, saved.workspace)?;
        let url = reqwest::Url::parse(webhook).context("Invalid event address")?;
        ensure!(
            url.scheme() == "https"
                || (self.api.is_fixture() && url.host_str() == Some("127.0.0.1")),
            "Pipedream events need an HTTPS address"
        );
        if !self.healthy(&binding).await? {
            return Err(Refusal::NeedsReconnect.into());
        }
        let component = self
            .component("triggers", trigger, None, &binding.app)
            .await?
            .context("Pipedream does not offer this event as Cuckoo can use it")?;
        let properties = &component.schema["properties"];
        let mut props = Map::new();
        let config = match config {
            Value::Null => &Map::new(),
            other => other.as_object().context("Expected event settings")?,
        };
        for (name, value) in config {
            let kind = properties[name]["type"].as_str().unwrap_or_default();
            let plain = ["string", "integer", "boolean"].contains(&kind);
            ensure!(
                plain && (value.is_string() || value.is_i64() || value.is_boolean()),
                "Pipedream's {trigger} does not take {name} as a plain value"
            );
            let value = match value {
                Value::String(_) if component.lists.contains(name) => json!([value]),
                _ => value.clone(),
            };
            props.insert(name.clone(), value);
        }
        for required in component.schema["required"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let name = required.as_str().unwrap_or_default();
            ensure!(
                props.contains_key(name),
                "Pipedream's {trigger} needs {name}"
            );
        }
        props.insert(
            component.app_prop.clone(),
            json!({"authProvisionId": binding.account}),
        );
        let deployed = self
            .api
            .request(
                Method::POST,
                "triggers/deploy",
                &[],
                Some(&json!({"id":trigger,"version":component.version,"external_user_id":binding.user,
                    "configured_props":props,"webhook_url":webhook,"emit_on_deploy":false})),
            )
            .await?;
        let deployed = &deployed["data"];
        let subscription = id(deployed["id"]
            .as_str()
            .context("Pipedream omitted the deployed trigger")?)?
        .to_owned();
        match deployed["webhook_signing_key"]
            .as_str()
            .filter(|key| !key.is_empty() && key.len() <= 512)
        {
            Some(key) => Ok(Subscription {
                id: subscription,
                signing_key: Some(key.to_owned()),
            }),
            None => {
                // Unsigned deliveries can't be trusted: don't leave the trigger running.
                let _ = self.remove_trigger(saved.workspace, &subscription).await;
                anyhow::bail!("Pipedream did not return a signing key for the event")
            }
        }
    }

    /// Deletes a deployed trigger. One that is already gone succeeds.
    pub(super) async fn remove_trigger(&self, workspace: &str, subscription: &str) -> Result<()> {
        let path = format!("deployed-triggers/{}", id(subscription)?);
        let user = user(workspace);
        match self
            .api
            .send(Method::DELETE, &path, &[("external_user_id", &user)], None)
            .await?
        {
            Reply::Ok(_) | Reply::Status(404) => Ok(()),
            Reply::Status(code) => {
                anyhow::bail!("Pipedream did not remove the event subscription (HTTP {code})")
            }
        }
    }
}

/// Verifies a delivery with the subscription's signing key, then reads it.
pub(super) fn parse(
    headers: &axum::http::HeaderMap,
    body: &[u8],
    secret: &str,
    now: i64,
) -> Result<Vec<Delivery>> {
    verify(headers, body, secret, now)?;
    let value: Value = serde_json::from_slice(body).context("Invalid event body")?;
    ensure!(value.is_object(), "Invalid event body");
    let (data, context) = match value.get("event") {
        Some(event) => (
            event.clone(),
            value.get("context").cloned().unwrap_or_default(),
        ),
        None => (value, Value::Null),
    };
    ensure!(
        context.is_null() || context.is_object(),
        "Invalid event context"
    );
    let event_id = match context.get("id") {
        Some(Value::String(event)) => {
            ensure!(
                !event.is_empty() && event.len() <= 250 && !event.chars().any(char::is_control),
                "Invalid event id"
            );
            event.clone()
        }
        None | Some(Value::Null) => format!(
            "sha256:{}",
            URL_SAFE_NO_PAD.encode(ring::digest::digest(&ring::digest::SHA256, body))
        ),
        Some(_) => anyhow::bail!("Invalid event id"),
    };
    let subscription = match context.get("emitter_id") {
        Some(Value::String(emitter)) => id(emitter)?.to_owned(),
        None | Some(Value::Null) => String::new(),
        Some(_) => anyhow::bail!("Invalid event emitter"),
    };
    Ok(vec![Delivery {
        event_id,
        user: context["external_user_id"]
            .as_str()
            .map(|u| u.chars().take(250).collect()),
        account: None,
        kind: DeliveryKind::Event {
            subscription,
            // The saved subscription knows its trigger; deliveries don't name it.
            trigger: String::new(),
            data,
        },
    }])
}

/// `x-pd-signature: t=…,v1=…` over `"{t}.{raw body}"`, within ±300 seconds,
/// compared in constant time. Several `v1` values may be sent; one must match.
fn verify(headers: &axum::http::HeaderMap, body: &[u8], secret: &str, now: i64) -> Result<()> {
    ensure!(
        !secret.is_empty() && secret.len() <= 512,
        "Event verification is unavailable"
    );
    ensure!(body.len() <= MAX_BODY, "The event is too large");
    let mut values = headers.get_all("x-pd-signature").iter();
    let header = values
        .next()
        .context("The event is not signed")?
        .to_str()
        .context("Invalid event signature")?;
    ensure!(values.next().is_none(), "Invalid event signature");
    let mut time = None;
    let mut signatures = vec![];
    for part in header.split(',').take(8) {
        match part.trim().split_once('=') {
            Some(("t", value)) => {
                ensure!(
                    time.is_none()
                        && !value.is_empty()
                        && value.len() <= 12
                        && value.bytes().all(|b| b.is_ascii_digit()),
                    "Invalid event signature"
                );
                time = Some(value);
            }
            Some(("v1", value)) => signatures.push(value),
            _ => {}
        }
    }
    let time_text = time.context("Invalid event signature")?;
    let time: i64 = time_text.parse().context("Invalid event signature")?;
    ensure!(
        (now - time).abs() <= WINDOW,
        "The event's signature is outside the accepted window"
    );
    let mut signed = format!("{time_text}.").into_bytes();
    signed.extend_from_slice(body);
    let key = hmac::Key::new(hmac::HMAC_SHA256, secret.as_bytes());
    ensure!(
        signatures
            .iter()
            .filter_map(|s| hex(s))
            .any(|s| hmac::verify(&key, &signed, &s).is_ok()),
        "Invalid event signature"
    );
    Ok(())
}

/// A 32-byte digest written as 64 hex digits.
fn hex(text: &str) -> Option<Vec<u8>> {
    if text.len() != 64 || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    (0..64)
        .step_by(2)
        .map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok())
        .collect()
}

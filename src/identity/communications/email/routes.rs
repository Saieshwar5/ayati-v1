//! Switching an agent's email on and off, and the two webhooks Postmark calls:
//! incoming mail and delivery events.
use super::settings::{self, EVENTS_PATH, INCOMING_PATH, Settings};
use crate::identity::{ApiResult, App, Error, Shared, identity, postgres::EmailOutcome};
use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Path as RoutePath, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::Value;

/// Postmark posts whole messages, attachments included, as JSON.
const MAIL_BYTES: usize = 12 * 1024 * 1024;

pub(crate) fn routes() -> Router<Shared> {
    let agent = Router::new()
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{agent}/email",
            get(status),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{agent}/email/activate",
            post(activate),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{agent}/email/deactivate",
            post(deactivate),
        )
        .layer(DefaultBodyLimit::max(4 * 1024));
    let hooks = Router::new()
        .route(INCOMING_PATH, post(incoming))
        .route(EVENTS_PATH, post(events))
        .layer(DefaultBodyLimit::max(MAIL_BYTES));
    agent.merge(hooks)
}

fn configured() -> ApiResult<Settings> {
    settings::current()
        .map_err(|e| {
            tracing::warn!(error = %e, "Email configuration is unavailable");
            Error(
                anyhow::anyhow!("Email is unavailable right now. Try again later."),
                StatusCode::SERVICE_UNAVAILABLE,
            )
        })?
        .ok_or_else(|| {
            Error(
                anyhow::anyhow!("This Cuckoo server doesn't offer email"),
                StatusCode::NOT_FOUND,
            )
        })
}
fn domain() -> Option<String> {
    settings::current().ok().flatten().map(|s| s.domain)
}

async fn status(
    State(app): State<Shared>,
    RoutePath((w, agent)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let domain = domain();
    Ok(Json(
        app.store
            .email_status(&user, &w, &agent, domain.as_deref())
            .await?,
    ))
}
async fn activate(
    State(app): State<Shared>,
    RoutePath((w, agent)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let s = configured()?;
    Ok(Json(
        app.store
            .email_activate(&user, &w, &agent, &s.domain)
            .await?,
    ))
}
async fn deactivate(
    State(app): State<Shared>,
    RoutePath((w, agent)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let domain = domain();
    Ok(Json(
        app.store
            .email_deactivate(&user, &w, &agent, domain.as_deref())
            .await?,
    ))
}

/// Postmark's Basic credentials, or the status that refuses the call.
fn verified(headers: &HeaderMap) -> Result<Settings, StatusCode> {
    let s = settings::current()
        .ok()
        .flatten()
        .ok_or(StatusCode::NOT_FOUND)?;
    let given = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());
    if !s.authorized(given) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(s)
}
fn database(e: &anyhow::Error) -> bool {
    e.chain().any(|c| c.is::<sea_orm::DbErr>())
}

/// Postmark retries until it gets 200, except after a 403. Mail Cuckoo can't
/// read is answered 403 so it isn't sent again; a database problem is 503.
async fn incoming(State(app): State<Shared>, headers: HeaderMap, body: Bytes) -> Response {
    let s = match verified(&headers) {
        Ok(s) => s,
        Err(status) => return status.into_response(),
    };
    let parsed = match super::inbound::parse(&body) {
        Ok(p) => p,
        Err(e) => {
            tracing::info!(error = %e, "An inbound email was refused");
            return StatusCode::FORBIDDEN.into_response();
        }
    };
    if !s.serves(&parsed.to) {
        return StatusCode::OK.into_response();
    }
    match receive(&app, &parsed).await {
        Ok(outcome) => {
            if outcome == EmailOutcome::Admitted {
                app.runtimes.chat_wake.notify_one();
            }
            StatusCode::OK.into_response()
        }
        Err(e) => {
            tracing::warn!(error = %e, "An inbound email could not be saved");
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
    }
}
async fn receive(app: &App, parsed: &super::inbound::Parsed) -> anyhow::Result<EmailOutcome> {
    match app.store.email_incoming(parsed).await {
        // Mail Cuckoo refuses (a changed retry, a malformed thread) is not retried.
        Err(e) if !database(&e) => {
            tracing::info!(error = %e, "An inbound email was refused");
            Ok(EmailOutcome::Ignored)
        }
        other => other,
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Event {
    record_type: String,
    #[serde(default, rename = "Type")]
    kind: String,
    #[serde(default)]
    inactive: bool,
    #[serde(default)]
    metadata: std::collections::HashMap<String, String>,
}

/// Delivery and bounce events for replies Cuckoo sent. The attempt key comes
/// back in the Metadata Cuckoo gave the send.
async fn events(State(app): State<Shared>, headers: HeaderMap, body: Bytes) -> Response {
    if let Err(status) = verified(&headers) {
        return status.into_response();
    }
    let Ok(event) = serde_json::from_slice::<Event>(&body) else {
        return StatusCode::FORBIDDEN.into_response();
    };
    let delivered = match event.record_type.as_str() {
        "Delivery" => true,
        // A soft bounce may still be delivered later; only a final one counts.
        "Bounce" if event.inactive || event.kind == "HardBounce" => false,
        _ => return StatusCode::OK.into_response(),
    };
    let (Some(w), Some(key)) = (
        event.metadata.get(super::postmark::WORKSPACE),
        event.metadata.get(super::postmark::KEY),
    ) else {
        return StatusCode::OK.into_response();
    };
    match app.store.email_receipt(w, key, delivered).await {
        // Retried by Postmark only when Cuckoo's database was unavailable.
        Err(e) if database(&e) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
        _ => StatusCode::OK.into_response(),
    }
}

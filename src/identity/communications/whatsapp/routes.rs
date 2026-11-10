//! Switching an agent's WhatsApp on and off, a person's linked number, and the
//! two webhooks Twilio calls: incoming messages and delivery updates.
use super::settings::{self, INCOMING_PATH, STATUS_PATH, Settings};
use crate::identity::{
    ApiResult, App, Error, Shared, identity,
    postgres::{NoFreeNumber, Offer, WhatsappIncoming, WhatsappOutcome},
};
use axum::{
    Form, Json, Router,
    extract::{DefaultBodyLimit, OriginalUri, Path as RoutePath, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::{Value, json};

pub(crate) fn routes() -> Router<Shared> {
    Router::new()
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{agent}/whatsapp",
            get(status),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{agent}/whatsapp/activate",
            post(activate),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{agent}/whatsapp/deactivate",
            post(deactivate),
        )
        .route("/api/identity/mobile", get(mobile))
        .route("/api/identity/mobile/link", post(link))
        .route("/api/identity/mobile/unlink", post(unlink))
        .route(INCOMING_PATH, post(incoming))
        .route(STATUS_PATH, post(delivery))
        .layer(DefaultBodyLimit::max(64 * 1024))
}

fn configured() -> ApiResult<Settings> {
    settings::current()
        .map_err(|e| {
            tracing::warn!(error = %e, "WhatsApp configuration is unavailable");
            Error(
                anyhow::anyhow!("WhatsApp is unavailable right now. Try again later."),
                StatusCode::SERVICE_UNAVAILABLE,
            )
        })?
        .ok_or_else(|| {
            Error(
                anyhow::anyhow!("This Cuckoo server doesn't offer WhatsApp"),
                StatusCode::NOT_FOUND,
            )
        })
}
fn offer(s: &Settings) -> Offer {
    Offer {
        numbers: s.numbers.clone(),
        cooling_days: s.cooling_days,
    }
}

async fn status(
    State(app): State<Shared>,
    RoutePath((w, agent)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let offered = super::offered();
    Ok(Json(
        app.store
            .whatsapp_status(&user, &w, &agent, offered)
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
    match app
        .store
        .whatsapp_activate(&user, &w, &agent, &offer(&s))
        .await
    {
        Ok(v) => Ok(Json(v)),
        Err(e) if e.is::<NoFreeNumber>() => Err(Error(e, StatusCode::CONFLICT)),
        Err(e) => Err(e.into()),
    }
}
async fn deactivate(
    State(app): State<Shared>,
    RoutePath((w, agent)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let cooling_days = settings::current()
        .ok()
        .flatten()
        .map_or(90, |s| s.cooling_days);
    let offer = Offer {
        numbers: vec![],
        cooling_days,
    };
    Ok(Json(
        app.store
            .whatsapp_deactivate(&user, &w, &agent, &offer)
            .await?,
    ))
}

async fn mobile(State(app): State<Shared>, headers: HeaderMap) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let mut value = app.store.mobile_status(&user).await?;
    value["offered"] = json!(super::offered());
    Ok(Json(value))
}
/// A code to send from the person's WhatsApp, and a Cuckoo number to send it
/// to (any Cuckoo number accepts link codes).
async fn link(State(app): State<Shared>, headers: HeaderMap) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let s = configured()?;
    let number = s.numbers.first().cloned().ok_or_else(|| {
        Error(
            anyhow::anyhow!("WhatsApp is unavailable right now. Try again later."),
            StatusCode::SERVICE_UNAVAILABLE,
        )
    })?;
    let mut value = app.store.mobile_link_code(&user).await?;
    let code = value["code"].as_str().unwrap_or_default().to_owned();
    value["send_to"] = json!(number);
    value["link"] = json!(format!(
        "https://wa.me/{}?text={code}",
        number.trim_start_matches('+')
    ));
    Ok(Json(value))
}
async fn unlink(State(app): State<Shared>, headers: HeaderMap) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.mobile_unlink(&user).await?))
}

/// Checks Twilio's signature over the exact URL it called and every parameter.
/// Returns the settings and the parameters as a lookup.
fn verified(
    uri: &axum::http::Uri,
    headers: &HeaderMap,
    params: &[(String, String)],
) -> Result<(Settings, impl Fn(&str) -> Option<String>), StatusCode> {
    let s = settings::current()
        .ok()
        .flatten()
        .ok_or(StatusCode::NOT_FOUND)?;
    let token = s.token().map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let path = uri.path_and_query().map_or(uri.path(), |p| p.as_str());
    let signature = headers
        .get("x-twilio-signature")
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;
    if !settings::signed(&token, &s.url(path), params, signature) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let params = params.to_vec();
    let lookup = move |name: &str| {
        params
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
    };
    Ok((s, lookup))
}

/// TwiML: the fixed reply, if any, that Twilio sends back to the person.
fn twiml(reply: Option<String>) -> Response {
    let body = match reply {
        Some(text) => format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><Response><Message>{}</Message></Response>",
            escape(&text)
        ),
        None => "<?xml version=\"1.0\" encoding=\"UTF-8\"?><Response></Response>".into(),
    };
    ([(header::CONTENT_TYPE, "text/xml")], body).into_response()
}
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
fn number(value: Option<String>) -> Option<String> {
    value?
        .strip_prefix("whatsapp:")
        .map(str::to_owned)
        .filter(|n| settings::valid_number(n))
}

async fn incoming(
    State(app): State<Shared>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    Form(params): Form<Vec<(String, String)>>,
) -> Response {
    let (s, param) = match verified(&uri, &headers, &params) {
        Ok(found) => found,
        Err(status) => return status.into_response(),
    };
    let (Some(from), Some(to), Some(event)) = (
        number(param("From")),
        number(param("To")),
        param("MessageSid").filter(|sid| crate::identity::identifier(sid).is_ok()),
    ) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if !s.offers(&to) {
        return twiml(None);
    }
    let text = param("Body").unwrap_or_default();
    if text.trim().is_empty() {
        let media = param("NumMedia").is_some_and(|n| n != "0");
        return twiml(media.then(|| "I can only read text messages for now.".to_owned()));
    }
    let input = WhatsappIncoming {
        event,
        from,
        to,
        text,
    };
    match receive(&app, &input).await {
        Ok((outcome, problem)) => {
            if outcome == WhatsappOutcome::Admitted {
                app.runtimes.chat_wake.notify_one();
            }
            twiml(outcome.reply(problem))
        }
        // Twilio retries; a repeat is recognised by its message ID.
        Err(e) => {
            tracing::warn!(error = %e, "A WhatsApp message could not be saved");
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
    }
}
async fn receive(
    app: &App,
    input: &WhatsappIncoming,
) -> anyhow::Result<(WhatsappOutcome, Option<&'static str>)> {
    match app.store.whatsapp_incoming(input).await {
        // A message Cuckoo refuses (too long, malformed) is not retried.
        Err(e) if !e.chain().any(|c| c.is::<sea_orm::DbErr>()) => {
            tracing::info!(error = %e, "A WhatsApp message was refused");
            Ok((WhatsappOutcome::Ignored, None))
        }
        other => other,
    }
}

async fn delivery(
    State(app): State<Shared>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    Form(params): Form<Vec<(String, String)>>,
) -> Response {
    let (_, param) = match verified(&uri, &headers, &params) {
        Ok(found) => found,
        Err(status) => return status.into_response(),
    };
    let query: Vec<(String, String)> = uri
        .query()
        .map(|q| {
            reqwest::Url::parse(&format!("http://x/?{q}"))
                .map(|u| u.query_pairs().into_owned().collect())
                .unwrap_or_default()
        })
        .unwrap_or_default();
    let get = |name: &str| {
        query
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
    };
    let (Some(w), Some(key)) = (get("workspace"), get("key")) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let delivered = match param("MessageStatus").as_deref() {
        Some("delivered" | "read") => true,
        Some("failed" | "undelivered") => false,
        // queued, sending, sent: nothing to record yet.
        _ => return StatusCode::NO_CONTENT.into_response(),
    };
    match app.store.whatsapp_receipt(&w, &key, delivered).await {
        // Retried by Twilio only when Cuckoo's database was unavailable.
        Err(e) if e.chain().any(|c| c.is::<sea_orm::DbErr>()) => {
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
        _ => StatusCode::NO_CONTENT.into_response(),
    }
}

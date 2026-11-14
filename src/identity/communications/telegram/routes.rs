//! A workspace's Telegram (connected once by an admin), an agent's Telegram
//! (switched on and off by its owner or an admin) and a person's linked
//! Telegram account. Telegram is read by `poller`; there are no webhooks.
use super::api::{Answer, Api};
use super::settings::{self, Settings};
use crate::identity::{
    ApiResult, Error, Shared, identity,
    postgres::{TelegramNotConnected, TelegramOwnerAsk},
};
use axum::{
    Json, Router,
    extract::{Path as RoutePath, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use serde_json::{Value, json};

pub(crate) fn routes() -> Router<Shared> {
    Router::new()
        .route("/api/identity/telegram", get(person))
        .route("/api/identity/telegram/link", post(link))
        .route("/api/identity/telegram/unlink", post(unlink))
        .route("/api/identity/workspaces/{w}/telegram", get(workspace))
        .route(
            "/api/identity/workspaces/{w}/telegram/connect",
            post(connect),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{agent}/telegram",
            get(status),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{agent}/telegram/activate",
            post(activate),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{agent}/telegram/deactivate",
            post(deactivate),
        )
}

fn configured() -> ApiResult<Settings> {
    settings::current()
        .map_err(|e| {
            tracing::warn!(error = %e, "Telegram configuration is unavailable");
            Error(
                anyhow::anyhow!("Telegram is unavailable right now. Try again later."),
                StatusCode::SERVICE_UNAVAILABLE,
            )
        })?
        .ok_or_else(|| {
            Error(
                anyhow::anyhow!("This Cuckoo server doesn't offer Telegram"),
                StatusCode::NOT_FOUND,
            )
        })
}
fn manager() -> String {
    settings::current()
        .ok()
        .flatten()
        .map(|s| s.manager_username)
        .unwrap_or_default()
}
fn start_link(s: &Settings, code: &str) -> String {
    format!("https://t.me/{}?start={code}", s.manager_username)
}

async fn person(State(app): State<Shared>, headers: HeaderMap) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let mut value = app.store.telegram_link_status(&user).await?;
    value["offered"] = json!(super::offered());
    Ok(Json(value))
}
/// A Start link that connects the person's Telegram account to Cuckoo.
async fn link(State(app): State<Shared>, headers: HeaderMap) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let s = configured()?;
    let code = app.store.telegram_link_code(&user).await?;
    Ok(Json(
        json!({"link": start_link(&s, &code), "expires_in": 900}),
    ))
}
async fn unlink(State(app): State<Shared>, headers: HeaderMap) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.telegram_unlink(&user).await?))
}

async fn workspace(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store
            .telegram_workspace(&user, &w, super::offered())
            .await?,
    ))
}
/// A Start link that makes the admin the workspace's Telegram owner.
async fn connect(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let s = configured()?;
    let code = app
        .store
        .telegram_connect_code(&user, &w)
        .await
        .map_err(|e| Error(e, StatusCode::FORBIDDEN))?;
    Ok(Json(
        json!({"link": start_link(&s, &code), "expires_in": 900}),
    ))
}

async fn status(
    State(app): State<Shared>,
    RoutePath((w, agent)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store
            .telegram_status(&user, &w, &agent, super::offered(), &manager())
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
    let activation = match app
        .store
        .telegram_activate(&user, &w, &agent, &s.manager_username)
        .await
    {
        Ok(a) => a,
        Err(e) if e.is::<TelegramNotConnected>() => return Err(Error(e, StatusCode::CONFLICT)),
        Err(e) => return Err(e.into()),
    };
    let mut view = activation.view;
    if let Some(ask) = activation.ask {
        view["owner_asked"] = json!(ask_owner(&s, &ask).await);
    }
    Ok(Json(view))
}
async fn deactivate(
    State(app): State<Shared>,
    RoutePath((w, agent)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store
            .telegram_deactivate(&user, &w, &agent, &manager())
            .await?,
    ))
}

/// Sends the owner a button in Telegram that creates the agent's bot.
async fn ask_owner(s: &Settings, ask: &TelegramOwnerAsk) -> bool {
    let Ok(token) = s.token() else {
        return false;
    };
    let button = json!({"text": "Create the agent's bot", "request_managed_bot": {
        "request_id": ask.request_id, "suggested_name": ask.name, "suggested_username": ask.username}});
    let markup =
        json!({"keyboard": [[button]], "resize_keyboard": true, "one_time_keyboard": true});
    let sent = Api::new(s.api())
        .send(&token, ask.telegram_id, &ask.text, false, Some(markup))
        .await;
    if !matches!(sent, Answer::Done(_)) {
        tracing::warn!("The workspace's Telegram owner could not be asked to create a bot");
    }
    matches!(sent, Answer::Done(_))
}

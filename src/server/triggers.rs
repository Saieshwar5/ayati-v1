use super::{ApiResult, WebState};
use anyhow::Context;
use axum::{
    Extension, Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Path, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

pub(super) fn routes() -> Router<WebState> {
    Router::new()
        .route("/api/triggers", get(list).post(create))
        .route("/api/triggers/{id}/state", post(set_state))
        .route("/api/triggers/{id}/edit", post(edit))
        .route("/api/triggers/{id}/runs", get(runs))
        .route("/api/connections/{id}/event-sources", get(sources))
        .route("/api/connections/{id}/event-types", get(types))
        .route(
            "/api/connections/{id}/event-sources/create",
            post(create_source),
        )
        .route(
            "/hooks/composio",
            post(webhook).layer(DefaultBodyLimit::max(64000)),
        )
}
fn platform(state: &WebState) -> anyhow::Result<Arc<crate::platform::Platform>> {
    state
        .agent
        .platform()
        .context("Business tools are unavailable")
}
async fn list(Extension(state): Extension<WebState>) -> ApiResult<Json<Value>> {
    let p = platform(&state)?;
    Ok(Json(
        json!({"triggers":p.triggers(state.agent.identity())?,"event_management_supported":!p.client.as_ref().is_some_and(|c|c.is_consumer()),"webhook_configured":std::env::var("COMPOSIO_WEBHOOK_SECRET").is_ok_and(|s|s.len()>=16)}),
    ))
}
async fn sources(
    Extension(state): Extension<WebState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    let p = platform(&state)?;
    Ok(Json(json!({"sources":p.event_sources(&id).await?})))
}
async fn types(
    Extension(state): Extension<WebState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    Ok(Json(
        json!({"types":platform(&state)?.event_types(&id).await?}),
    ))
}
async fn create_source(
    Extension(state): Extension<WebState>,
    Path(id): Path<String>,
    Json(input): Json<crate::platform::event_sources::NewEventSource>,
) -> ApiResult<Json<Value>> {
    Ok(Json(
        platform(&state)?.create_event_source(&id, &input).await?,
    ))
}
async fn create(
    Extension(state): Extension<WebState>,
    Json(input): Json<crate::platform::triggers::NewTrigger>,
) -> ApiResult<Json<Value>> {
    platform(&state)?
        .add_trigger(state.agent.identity(), &input)
        .await?;
    Ok(Json(json!({"id":input.id})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Change {
    state: String,
    expected_revision: i64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Edit {
    name: String,
    instructions: String,
    expected_revision: i64,
}
async fn edit(
    Extension(state): Extension<WebState>,
    Path(id): Path<String>,
    Json(input): Json<Edit>,
) -> ApiResult<Json<Value>> {
    let p = platform(&state)?;
    p.edit_trigger(
        state.agent.identity(),
        &id,
        &input.name,
        &input.instructions,
        input.expected_revision,
    )
    .await?;
    p.fence_trigger_jobs(state.agent.identity(), &state.agent.store)?;
    Ok(Json(json!({"ok":true})))
}
async fn set_state(
    Extension(state): Extension<WebState>,
    Path(id): Path<String>,
    Json(input): Json<Change>,
) -> ApiResult<Json<Value>> {
    let p = platform(&state)?;
    p.set_trigger_state(
        state.agent.identity(),
        &id,
        &input.state,
        input.expected_revision,
    )
    .await?;
    p.fence_trigger_jobs(state.agent.identity(), &state.agent.store)?;
    Ok(Json(json!({"ok":true})))
}
async fn runs(
    Extension(state): Extension<WebState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    let p = platform(&state)?;
    let mut runs = p.trigger_runs(state.agent.identity(), &id)?;
    for run in runs.as_array_mut().unwrap() {
        if let Some(id) = run["task_id"].as_str() {
            let task = state.agent.store.task(id)?;
            run["task"] = json!(task);
        }
    }
    Ok(Json(json!({"runs":runs})))
}
async fn webhook(
    State(state): State<WebState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, StatusCode> {
    let value = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .ok_or(StatusCode::UNAUTHORIZED)
    };
    let secret =
        std::env::var("COMPOSIO_WEBHOOK_SECRET").map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let id = value("webhook-id")?;
    crate::platform::webhook::verify(
        &secret,
        id,
        value("webhook-timestamp")?,
        value("webhook-signature")?,
        &body,
        crate::storage::intake::now_ms() / 1000,
    )
    .map_err(|_| StatusCode::UNAUTHORIZED)?;
    let p = platform(&state).map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let _gate = p.gate.read().await;
    let accepted = p
        .accept_webhook(id, &body)
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    Ok(Json(json!({"accepted":accepted})))
}

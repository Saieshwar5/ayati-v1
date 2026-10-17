use super::{ApiResult, WebState};
use anyhow::Context;
use axum::response::{Html, IntoResponse, Response};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, State},
    http::HeaderMap,
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

pub(super) fn routes() -> Router<WebState> {
    Router::new()
        .route("/api/apps", get(list))
        .route("/api/apps/{id}/session", post(session))
        .route("/api/apps/{id}/document/{document}", get(document))
        .route("/api/apps/{id}/activate", post(activate))
        .route("/api/apps/{id}/archive", post(archive))
        .route("/api/apps/{id}/grants", get(grants).post(grant))
        .route(
            "/api/apps/{id}/records",
            post(records).layer(DefaultBodyLimit::max(12000)),
        )
}
async fn document(
    State(state): State<WebState>,
    Path((id, document)): Path<(String, String)>,
) -> ApiResult<Response> {
    let html = platform(&state)?.app_document(&id, &document)?;
    let mut response = Html(html).into_response();
    response
        .headers_mut()
        .insert("x-ayati-app-document", "1".parse().unwrap());
    Ok(response)
}
fn platform(state: &WebState) -> anyhow::Result<Arc<crate::platform::Platform>> {
    state
        .agent
        .platform()
        .context("Apps require Goose business services")
}
async fn list(State(state): State<WebState>) -> ApiResult<Json<Value>> {
    let Some(p) = state.agent.platform() else {
        return Ok(Json(json!({"apps":[],"available":false})));
    };
    Ok(Json(
        json!({"apps":p.apps(None)?,"available":true,"hosting":"local"}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Session {
    revision: i64,
    preview: bool,
}
async fn session(
    State(state): State<WebState>,
    Path(id): Path<String>,
    Json(input): Json<Session>,
) -> ApiResult<Json<Value>> {
    Ok(Json(
        platform(&state)?
            .app_session(&id, input.revision, input.preview)
            .await?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Activate {
    revision: i64,
    preview_session: String,
    expected_active: Option<i64>,
}
async fn activate(
    State(state): State<WebState>,
    Path(id): Path<String>,
    Json(input): Json<Activate>,
) -> ApiResult<Json<Value>> {
    platform(&state)?
        .activate_app(
            &id,
            input.revision,
            &input.preview_session,
            input.expected_active,
        )
        .await?;
    Ok(Json(json!({"ok":true})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Revision {
    revision: i64,
}
async fn archive(
    State(state): State<WebState>,
    Path(id): Path<String>,
    Json(input): Json<Revision>,
) -> ApiResult<Json<Value>> {
    let p = platform(&state)?;
    let agents = p.app_agents(&id)?;
    p.archive_app(&id, input.revision).await?;
    if let Some(workspace) = &state.workspace {
        for id in agents {
            if let Ok((agent, _, _)) = workspace.agent(&id) {
                agent.invalidate_business_context().await?;
            }
        }
    }
    Ok(Json(json!({"ok":true})))
}
async fn records(
    State(state): State<WebState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(input): Json<crate::platform::app_records::RecordCommand>,
) -> ApiResult<Json<Value>> {
    let token = headers
        .get("x-ayati-app-session")
        .and_then(|h| h.to_str().ok())
        .context("Open an app session before accessing records")?;
    Ok(Json(
        platform(&state)?
            .session_records(&id, token, &input)
            .await?,
    ))
}
async fn grants(State(state): State<WebState>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    Ok(Json(json!({"grants":platform(&state)?.app_grants(&id)?})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Grant {
    agent_id: String,
    collection: String,
    enabled: bool,
    can_write: bool,
}
async fn grant(
    State(state): State<WebState>,
    Path(id): Path<String>,
    Json(input): Json<Grant>,
) -> ApiResult<Json<Value>> {
    let workspace = state.workspace.as_ref().context("Workspace unavailable")?;
    let (agent, _, _) = workspace.agent(&input.agent_id)?;
    platform(&state)?
        .set_app_grant(
            &id,
            &input.agent_id,
            &input.collection,
            input.enabled,
            input.can_write,
        )
        .await?;
    if !input.enabled {
        agent.invalidate_business_context().await?;
    }
    Ok(Json(json!({"ok":true})))
}

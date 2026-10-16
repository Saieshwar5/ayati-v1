use super::{ApiResult, WebState};
use crate::workspace::registry::NewAgent;
use anyhow::{Context, Result, ensure};
use axum::{
    Json, Router,
    body::Body,
    extract::{Path, State},
    http::Request,
    routing::{get, post},
};
use serde_json::{Value, json};

pub(super) fn routes() -> Router<WebState> {
    Router::new()
        .route("/api/agents", get(list).post(create))
        .route("/api/agents/{id}/retry", post(retry))
}
// These routes manage the local owner's whole workspace, independent of selection.
async fn list(State(state): State<WebState>) -> ApiResult<Json<Value>> {
    Ok(Json(match &state.workspace {
        Some(workspace) => workspace.snapshot()?,
        None => json!({"agents":[],"creation_available":false}),
    }))
}
async fn create(
    State(state): State<WebState>,
    Json(input): Json<NewAgent>,
) -> ApiResult<Json<Value>> {
    Ok(Json(json!(
        state
            .workspace
            .context("Workspace unavailable")?
            .create(input)
            .await?
    )))
}
async fn retry(
    State(state): State<WebState>,
    Path(id): Path<String>,
    Json(input): Json<serde_json::Map<String, Value>>,
) -> ApiResult<Json<Value>> {
    if !input.is_empty() {
        return Err(anyhow::anyhow!("Retry accepts an empty object only").into());
    }
    Ok(Json(json!(
        state
            .workspace
            .context("Workspace unavailable")?
            .retry(&id)
            .await?
    )))
}

pub(super) fn select(state: &WebState, request: &Request<Body>) -> Result<WebState> {
    if !request.uri().path().starts_with("/api/") {
        return Ok(state.clone());
    }
    let header = request
        .headers()
        .get("x-ayati-agent")
        .map(|h| h.to_str())
        .transpose()?;
    // EventSource and download links cannot set custom headers. No other endpoint
    // accepts query identity, avoiding hidden defaults or conflicting selectors.
    let url = reqwest::Url::parse(&format!("http://localhost{}", request.uri()))?;
    let ids: Vec<_> = url
        .query_pairs()
        .filter(|(key, _)| key == "agent")
        .map(|(_, v)| v.into_owned())
        .collect();
    ensure!(ids.len() <= 1, "Ambiguous agent selection");
    let query = ids.first().map(String::as_str);
    ensure!(
        query.is_none()
            || request.uri().path() == "/api/events"
            || request.uri().path().starts_with("/api/files/"),
        "Agent query is unsupported on this endpoint"
    );
    ensure!(
        header.is_none() || query.is_none() || header == query,
        "Conflicting agent selection"
    );
    let id = header.or(query).unwrap_or("local-agent");
    let mut selected = state.clone();
    if let Some(workspace) = &state.workspace {
        let (agent, model, ready) = workspace.agent(id)?;
        selected.model_ready = ready;
        selected.agent = agent;
        selected.model_name = model;
    } else {
        ensure!(id == "local-agent", "Unknown agent");
    }
    Ok(selected)
}

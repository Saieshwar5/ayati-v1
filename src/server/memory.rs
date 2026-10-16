use super::{ApiResult, WebState};
use crate::storage::memory::MemoryScope;
use axum::{
    Json, Router,
    extract::{Path, Query},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};

pub(super) fn routes() -> Router<WebState> {
    Router::new()
        .route("/api/memory", get(search).post(save))
        .route("/api/memory/{id}/forget", post(forget))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Search {
    #[serde(default)]
    query: String,
}
async fn search(
    axum::Extension(state): axum::Extension<WebState>,
    Query(input): Query<Search>,
) -> ApiResult<Json<Value>> {
    Ok(Json(
        json!({"enabled":state.agent.memory_enabled(),"memories":state.agent.store.search_memory(&MemoryScope::local(),&input.query)?}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Note {
    key: String,
    content: String,
    replaces: Option<String>,
}
async fn save(
    axum::Extension(state): axum::Extension<WebState>,
    Json(input): Json<Note>,
) -> ApiResult<Json<Value>> {
    Ok(Json(
        state
            .agent
            .edit_memory(Some(&input.key), Some(&input.content), input.replaces, None)
            .await?,
    ))
}
async fn forget(
    axum::Extension(state): axum::Extension<WebState>,
    Path(id): Path<String>,
    Json(input): Json<serde_json::Map<String, Value>>,
) -> ApiResult<Json<Value>> {
    if !input.is_empty() {
        return Err(anyhow::anyhow!("Forget accepts an empty object only").into());
    }
    Ok(Json(
        state.agent.edit_memory(None, None, None, Some(&id)).await?,
    ))
}

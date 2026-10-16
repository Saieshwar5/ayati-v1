use super::{ApiResult, WebState};
use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};

pub fn routes() -> Router<WebState> {
    Router::new()
        .route("/api/home", get(home))
        .route("/api/tasks/{id}/review", post(review))
        .route("/api/tasks/{id}/history", get(history))
}

async fn home(State(state): State<WebState>) -> ApiResult<Json<Value>> {
    let mut agents = Vec::new();
    if let Some(workspace) = &state.workspace {
        for profile in workspace.profiles()? {
            let attention = if profile.state == "ready" {
                Some(workspace.agent(&profile.id)?.0.store.attention()?)
            } else {
                None
            };
            agents.push(json!({"profile":profile,"attention":attention}));
        }
    } else {
        agents.push(json!({"profile":{"id":"local-agent","name":"Ayati","state":"ready","detail":""},"attention":state.agent.store.attention()?}));
    }
    Ok(Json(json!({"agents":agents})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Review {}
async fn review(
    Extension(state): Extension<WebState>,
    Path(id): Path<String>,
    Json(_): Json<Review>,
) -> ApiResult<Json<Value>> {
    state.agent.store.review_outcome(&id)?;
    Ok(Json(json!({"ok":true})))
}

async fn history(
    Extension(state): Extension<WebState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    let task = state
        .agent
        .store
        .task(&id)?
        .ok_or_else(|| anyhow::anyhow!("Request does not exist"))?;
    Ok(Json(
        json!({"task":task,"messages":state.agent.store.task_messages(&id)?}),
    ))
}

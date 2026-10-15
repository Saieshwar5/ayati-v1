use super::{ApiResult, WebState};
use crate::storage::intake::{LocalEvent, NewSchedule};
use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};

pub(super) fn routes() -> Router<WebState> {
    Router::new()
        .route("/api/intake/events", post(event))
        .route("/api/schedules", get(schedules).post(add_schedule))
        .route("/api/schedules/{id}/enabled", post(set_enabled))
}

async fn event(
    State(state): State<WebState>,
    Json(event): Json<LocalEvent>,
) -> ApiResult<Json<Value>> {
    Ok(Json(json!(state.agent.accept_event(&event)?)))
}

async fn schedules(State(state): State<WebState>) -> ApiResult<Json<Value>> {
    Ok(Json(json!({"schedules":state.agent.store.schedules()?})))
}

async fn add_schedule(
    State(state): State<WebState>,
    Json(input): Json<NewSchedule>,
) -> ApiResult<Json<Value>> {
    Ok(Json(json!(state.agent.add_schedule(&input)?)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Enabled {
    enabled: bool,
}

async fn set_enabled(
    State(state): State<WebState>,
    Path(id): Path<String>,
    Json(input): Json<Enabled>,
) -> ApiResult<Json<Value>> {
    state.agent.set_schedule_enabled(&id, input.enabled)?;
    Ok(Json(json!({"ok":true})))
}

use super::{ApiResult, WebState};
use axum::{
    Json, Router,
    extract::State,
    routing::{get, post},
};
use serde_json::Value;

pub(super) fn routes() -> Router<WebState> {
    Router::new()
        .route("/api/browser/status", get(status))
        .route("/api/browser/control", post(control))
}
async fn status(State(state): State<WebState>) -> Json<Value> {
    Json(state.agent.browser_status().await)
}
async fn control(
    State(state): State<WebState>,
    Json(request): Json<Value>,
) -> ApiResult<Json<Value>> {
    Ok(Json(state.agent.browser_request(request).await?))
}

use super::{ApiResult, WebState};
use axum::{
    Json, Router,
    routing::{get, post},
};
use serde_json::Value;

pub(super) fn routes() -> Router<WebState> {
    Router::new()
        .route("/api/browser/status", get(status))
        .route("/api/browser/control", post(control))
}
async fn status(axum::Extension(state): axum::Extension<WebState>) -> Json<Value> {
    Json(state.agent.browser_status().await)
}
async fn control(
    axum::Extension(state): axum::Extension<WebState>,
    Json(request): Json<Value>,
) -> ApiResult<Json<Value>> {
    Ok(Json(state.agent.browser_request(request).await?))
}

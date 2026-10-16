use super::{ApiResult, WebState};
use axum::{
    Json, Router,
    extract::State,
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::Value;

pub fn routes() -> Router<WebState> {
    Router::new()
        .route("/api/setup", get(status))
        .route("/api/setup/test", post(test))
}
async fn status(State(state): State<WebState>) -> ApiResult<Json<Value>> {
    let workspace = state
        .workspace
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Workspace unavailable"))?;
    Ok(Json(workspace.setup_status().await))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TestRequest {
    confirm: bool,
}
async fn test(
    State(state): State<WebState>,
    Json(input): Json<TestRequest>,
) -> ApiResult<Json<Value>> {
    if !input.confirm {
        return Err(anyhow::anyhow!("Confirm the external model request before testing").into());
    }
    let workspace = state
        .workspace
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Workspace unavailable"))?;
    Ok(Json(workspace.test_provider().await?))
}

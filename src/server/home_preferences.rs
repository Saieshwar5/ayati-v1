use super::{ApiResult, WebState};
use crate::platform::home_preferences::HomePreferences;
use anyhow::Context;
use axum::{Json, Router, extract::State, routing::get};
use serde_json::{Value, json};
pub(super) fn routes() -> Router<WebState> {
    Router::new().route("/api/home/preferences", get(read).post(save))
}
async fn read(State(state): State<WebState>) -> ApiResult<Json<Value>> {
    let p = state
        .agent
        .platform()
        .context("Business clock requires Goose")?;
    Ok(Json(json!({"preferences":p.home_preferences()?})))
}
async fn save(
    State(state): State<WebState>,
    Json(input): Json<HomePreferences>,
) -> ApiResult<Json<Value>> {
    state
        .agent
        .platform()
        .context("Business clock requires Goose")?
        .save_home_preferences(&input)?;
    Ok(Json(json!({"preferences":input})))
}

//! Admin-only workspace configuration; personal preferences use existing profile APIs.
use super::*;
use axum::{Json, extract::Path as RoutePath, http::HeaderMap};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Update {
    name: String,
    previous_name: String,
}
pub(super) async fn read(
    State(app): State<Shared>,
    headers: HeaderMap,
    RoutePath(w): RoutePath<String>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.workspace_settings(&user, &w).await?))
}
pub(super) async fn update(
    State(app): State<Shared>,
    headers: HeaderMap,
    RoutePath(w): RoutePath<String>,
    Json(input): Json<Update>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let name = input.name.trim();
    check(
        !name.is_empty() && name.len() <= 80 && !name.chars().any(char::is_control),
        "Use a workspace name between 1 and 80 characters",
    )?;
    app.store
        .rename_workspace(&user, &w, name, &input.previous_name)
        .await?;
    Ok(Json(json!({"name":name})))
}
pub(super) async fn activity(
    State(app): State<Shared>,
    headers: HeaderMap,
    RoutePath(w): RoutePath<String>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.workspace_activity(&user, &w).await?))
}

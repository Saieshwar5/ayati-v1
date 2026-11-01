pub(crate) mod direct;
pub(crate) mod maintenance;
pub(crate) mod model;
use super::*;
use axum::{Json, extract::Path as RoutePath};
use serde_json::Value;
pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route("/api/identity/workspaces/{w}/home-panels", get(list))
        .route(
            "/api/identity/workspaces/{w}/home-panels/layout",
            post(layout),
        )
        .route(
            "/api/identity/workspaces/{w}/home-panels/{panel}/review",
            post(review),
        )
        .route(
            "/api/identity/workspaces/{w}/home-panels/{panel}/maintenance",
            post(agreement),
        )
        .route(
            "/api/identity/workspaces/{w}/home-panels/{panel}/refresh",
            post(refresh),
        )
}
async fn agreement(
    State(app): State<Shared>,
    RoutePath((w, panel)): RoutePath<(String, String)>,
    headers: axum::http::HeaderMap,
    Json(input): Json<maintenance::Agreement>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store
            .save_home_agreement(&user, &w, &panel, &input)
            .await?,
    ))
}
async fn refresh(
    State(app): State<Shared>,
    RoutePath((w, panel)): RoutePath<(String, String)>,
    headers: axum::http::HeaderMap,
    Json(input): Json<maintenance::Update>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store
            .update_home_panel(&user, &w, &panel, &input)
            .await?,
    ))
}
async fn list(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let pg = &app.store;
    Ok(Json(pg.home_panels(&user, &w).await?))
}
async fn layout(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: axum::http::HeaderMap,
    Json(input): Json<model::Layout>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.save_home_layout(&user, &w, &input).await?))
}
async fn review(
    State(app): State<Shared>,
    RoutePath((w, panel)): RoutePath<(String, String)>,
    headers: axum::http::HeaderMap,
    Json(input): Json<model::Review>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store
            .review_home_panel(&user, &w, &panel, &input)
            .await?,
    ))
}

pub(crate) mod model;
use super::*;
use axum::{Json, extract::Path as RoutePath};
use serde_json::{Value, json};
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
}
async fn list(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let database::Database::Postgres(pg) = &app.store else {
        return Ok(Json(json!({"available":false,"panels":[]})));
    };
    Ok(Json(pg.home_panels(&user, &w).await?))
}
async fn layout(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: axum::http::HeaderMap,
    Json(input): Json<model::Layout>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        super::collaboration::pg(&app)?
            .save_home_layout(&user, &w, &input)
            .await?,
    ))
}
async fn review(
    State(app): State<Shared>,
    RoutePath((w, panel)): RoutePath<(String, String)>,
    headers: axum::http::HeaderMap,
    Json(input): Json<model::Review>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        super::collaboration::pg(&app)?
            .review_home_panel(&user, &w, &panel, &input)
            .await?,
    ))
}

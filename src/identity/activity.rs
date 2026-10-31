//! Activity (D235): every agent request a person may see, needs-me items, agent
//! states and totals. Reading never starts work; decisions use the approval and
//! question routes.
use super::*;
use crate::identity::postgres::activity::Filters as ActivityFilters;
use axum::{
    Json,
    extract::{Path as RoutePath, Query},
    http::HeaderMap,
};
use serde::Deserialize;
use serde_json::{Value, json};

pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route(
            "/api/identity/workspaces/{w}/activity/history",
            get(history),
        )
        .route("/api/identity/workspaces/{w}/activity/totals", get(totals))
        .route("/api/identity/workspaces/{w}/activity/export", get(export))
        .route(
            "/api/identity/workspaces/{w}/activity/changes",
            get(changes),
        )
        .route("/api/identity/workspaces/{w}/activity/now", get(now_view))
        .route("/api/identity/workspaces/{w}/activity/seen", post(seen))
        .route(
            "/api/identity/workspaces/{w}/activity/requests/{reply}",
            get(request),
        )
}
async fn history(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Query(filters): Query<ActivityFilters>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.activity_history(&user, &w, &filters).await?))
}
async fn totals(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Query(filters): Query<ActivityFilters>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.activity_totals(&user, &w, &filters).await?))
}
/// History as a CSV file for the same filters (at most 5,000 requests).
async fn export(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Query(filters): Query<ActivityFilters>,
) -> ApiResult<axum::response::Response> {
    use axum::response::IntoResponse;
    let user = identity(&app, &headers).await?;
    let (csv, more) = app.store.activity_export(&user, &w, &filters).await?;
    Ok((
        [
            (axum::http::header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (
                axum::http::header::CONTENT_DISPOSITION,
                "attachment; filename=\"cuckoo-activity.csv\"",
            ),
            (
                axum::http::header::HeaderName::from_static("x-cuckoo-truncated"),
                if more { "true" } else { "false" },
            ),
        ],
        csv,
    )
        .into_response())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChangePage {
    agent: Option<String>,
    before: Option<i64>,
}
async fn changes(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Query(page): Query<ChangePage>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store
            .activity_changes(&user, &w, page.agent.as_deref(), page.before)
            .await?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NowPage {
    /// The start of the person's day, in ms, from their device.
    today: i64,
}
async fn now_view(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Query(page): Query<NowPage>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    check(page.today > 0, "Invalid day")?;
    Ok(Json(app.store.activity_now(&user, &w, page.today).await?))
}
async fn seen(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let mut value = app.store.activity_seen(&user, &w).await?;
    value["saved"] = json!(true);
    Ok(Json(value))
}
async fn request(
    State(app): State<Shared>,
    RoutePath((w, reply)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.activity_request(&user, &w, &reply).await?))
}

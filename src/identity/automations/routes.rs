use super::super::*;
use super::model::{Change, Schedule};
use axum::{Json, extract::Path as RoutePath, http::HeaderMap};
use serde_json::Value;
pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{a}/schedules",
            get(list).post(create),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{a}/schedules/{id}",
            get(details),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{a}/schedules/{id}/state",
            post(change),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{a}/schedules/{id}/runs",
            get(runs),
        )
}
async fn list(
    State(app): State<Shared>,
    RoutePath((w, a)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.schedules(&user, &w, &a).await?))
}
async fn create(
    State(app): State<Shared>,
    RoutePath((w, a)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<Schedule>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.save_schedule(&user, &w, &a, &input).await?))
}
async fn change(
    State(app): State<Shared>,
    RoutePath((w, a, id)): RoutePath<(String, String, String)>,
    headers: HeaderMap,
    Json(input): Json<Change>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store
            .change_schedule(&user, &w, &a, &id, &input)
            .await?,
    ))
}
async fn runs(
    State(app): State<Shared>,
    RoutePath((w, a, id)): RoutePath<(String, String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.schedule_runs(&user, &w, &a, &id).await?))
}

async fn details(
    State(app): State<Shared>,
    RoutePath((w, a, id)): RoutePath<(String, String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.schedule_details(&user, &w, &a, &id).await?))
}

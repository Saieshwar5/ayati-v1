//! Agent tasks and calendar (designs/agent-tasks): Activity › Calendar, pausing,
//! resuming and skipping a task's next run, and the workspace calendar in
//! Settings › General (working days, holidays, what a run due on a day off does).
//! Tasks are made and changed by asking the agent in chat; these routes only look
//! and stop.
use super::*;
use axum::{
    Json,
    extract::{Path as RoutePath, Query},
    http::HeaderMap,
};
use serde::Deserialize;
use serde_json::Value;

pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route(
            "/api/identity/workspaces/{w}/activity/calendar",
            get(calendar),
        )
        .route(
            "/api/identity/workspaces/{w}/activity/tasks/{id}/pause",
            post(pause),
        )
        .route(
            "/api/identity/workspaces/{w}/activity/tasks/{id}/skip",
            post(skip),
        )
        .route(
            "/api/identity/workspaces/{w}/settings/calendar",
            get(workspace_calendar).post(save_workspace_calendar),
        )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CalendarPage {
    from: i64,
    to: i64,
    scope: Option<String>,
}
async fn calendar(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Query(page): Query<CalendarPage>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store
            .activity_calendar(&user, &w, page.from, page.to, page.scope.as_deref())
            .await?,
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Pause {
    paused: bool,
}
async fn pause(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<Pause>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store
            .task_set_paused(&user, &w, &id, input.paused)
            .await?,
    ))
}
async fn skip(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.task_skip_next(&user, &w, &id).await?))
}

async fn workspace_calendar(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.workspace_calendar(&user, &w).await?))
}
async fn save_workspace_calendar(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Json(input): Json<crate::identity::postgres::WorkspaceCalendar>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store.save_workspace_calendar(&user, &w, &input).await?,
    ))
}

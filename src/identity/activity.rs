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
        .route(
            "/api/identity/workspaces/{w}/activity/requests/{reply}/retry",
            post(retry),
        )
        .route(
            "/api/identity/workspaces/{w}/activity/unhandled",
            get(unhandled),
        )
        .route("/api/identity/workspaces/{w}/activity/tasks", get(tasks))
        .route(
            "/api/identity/workspaces/{w}/activity/tasks/{id}",
            get(task),
        )
        .route(
            "/api/identity/workspaces/{w}/activity/tasks/{id}/close",
            post(close_task),
        )
        .route(
            "/api/identity/workspaces/{w}/activity/requests/{reply}/task",
            post(make_task),
        )
        .route(
            "/api/identity/workspaces/{w}/activity/tasks/{id}/follow-up/cancel",
            post(cancel_follow_up),
        )
        .route(
            "/api/identity/workspaces/{w}/activity/needs/{id}/dismiss",
            post(dismiss_need),
        )
        .route(
            "/api/identity/workspaces/{w}/activity/requests/{reply}/steer",
            post(steer_request),
        )
        .route(
            "/api/identity/workspaces/{w}/activity/tasks/{id}/steer",
            post(steer_task),
        )
        .route(
            "/api/identity/workspaces/{w}/activity/routine-proposals/{id}",
            post(decide_routine),
        )
        .merge(super::activity_review::routes())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Steer {
    text: String,
}
/// Add to a running request (the person who asked); the agent reads it next step.
async fn steer_request(
    State(app): State<Shared>,
    RoutePath((w, reply)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<Steer>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store
            .steer_request(&user, &w, &reply, &input.text)
            .await?,
    ))
}
/// Add to the request a task is running now.
async fn steer_task(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<Steer>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store.steer_task(&user, &w, &id, &input.text).await?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RoutineDecision {
    save: bool,
}
/// Save or dismiss a routine an agent proposed in Chat (the person who asked).
async fn decide_routine(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<RoutineDecision>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store
            .decide_schedule_draft(&user, &w, &id, input.save)
            .await?,
    ))
}
/// Coming up: stop a booked follow-up (the person it's for, managers, admins).
async fn cancel_follow_up(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    app.store.task_cancel_follow_up(&user, &w, &id).await?;
    Ok(Json(serde_json::json!({"cancelled": true})))
}
/// Needs fixing: dismiss something an agent reported (owners, managers, admins).
async fn dismiss_need(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    app.store.dismiss_need(&user, &w, &id).await?;
    Ok(Json(serde_json::json!({"dismissed": true})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskList {
    scope: Option<String>,
}
/// Activity → Open: the open tasks a person may see, what needs a person first.
async fn tasks(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Query(page): Query<TaskList>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store
            .tasks_open(&user, &w, page.scope.as_deref())
            .await?,
    ))
}
async fn task(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.task_details(&user, &w, &id).await?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CloseTask {
    outcome: String,
    result: Option<String>,
}
/// Close a task: done or cancelled, or "Not a task".
async fn close_task(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<CloseTask>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store
            .task_close(&user, &w, &id, &input.outcome, input.result.as_deref())
            .await?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MakeTask {
    title: Option<String>,
}
/// "Make this a task" on a request the person can see.
async fn make_task(
    State(app): State<Shared>,
    RoutePath((w, reply)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<MakeTask>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store
            .task_from_request(&user, &w, &reply, input.title.as_deref())
            .await?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UnhandledPage {
    from: Option<i64>,
    before: Option<i64>,
    scope: Option<String>,
    agent: Option<String>,
}
/// Events that arrived but no agent acted on, with the recorded reason.
async fn unhandled(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Query(page): Query<UnhandledPage>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store
            .activity_unhandled(
                &user,
                &w,
                page.from,
                page.before,
                page.scope.as_deref(),
                page.agent.as_deref(),
            )
            .await?,
    ))
}
/// Send a failed request's message again, as a new request quoting the failed
/// reply. Only the person who asked; never when an action's outcome is unknown.
async fn retry(
    State(app): State<Shared>,
    RoutePath((w, reply)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.request_retry(&user, &w, &reply).await?))
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
    /// `mine` (agents the person manages and their own requests) or `all`.
    scope: Option<String>,
}
async fn now_view(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Query(page): Query<NowPage>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    check(page.today > 0, "Invalid day")?;
    Ok(Json(
        app.store
            .activity_now(&user, &w, page.today, page.scope.as_deref())
            .await?,
    ))
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

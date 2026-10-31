//! Settings (D232): the Workspace page and its deletion, personal preferences,
//! "Report a problem" and the audit log. The server checks every role.
use super::*;
use crate::identity::postgres::settings::pause::PauseInput;
use crate::identity::postgres::settings::{
    preferences::Preferences, problems::ProblemReport, workspace::WorkspaceSettingsInput,
};
use axum::{
    Json,
    extract::{Path as RoutePath, Query},
    http::HeaderMap,
};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Update {
    name: String,
    previous_name: String,
}
async fn read(
    State(app): State<Shared>,
    headers: HeaderMap,
    RoutePath(w): RoutePath<String>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.workspace_settings(&user, &w).await?))
}
async fn update(
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
pub(super) fn routes() -> Router<Shared> {
    let base = "/api/identity/workspaces/{id}";
    Router::new()
        .route(&format!("{base}/settings"), get(read).post(update))
        .route(&format!("{base}/settings/workspace"), post(save_workspace))
        .route(
            &format!("{base}/settings/deletion"),
            post(schedule_deletion),
        )
        .route(
            &format!("{base}/settings/deletion/cancel"),
            post(cancel_deletion),
        )
        .route(
            &format!("{base}/preferences"),
            get(preferences).post(save_preferences),
        )
        .route(&format!("{base}/problem-reports"), post(report_problem))
        .route(&format!("{base}/activity"), get(activity))
        .route(
            &format!("{base}/agents/pause"),
            get(pause_state).post(pause),
        )
        .route(&format!("{base}/agents/resume"), post(resume))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AuditPage {
    kind: Option<String>,
    before: Option<i64>,
}
async fn activity(
    State(app): State<Shared>,
    headers: HeaderMap,
    RoutePath(w): RoutePath<String>,
    Query(page): Query<AuditPage>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let kind = page.kind.as_deref().unwrap_or("all");
    Ok(Json(
        app.store
            .workspace_activity(&user, &w, kind, page.before)
            .await?,
    ))
}
async fn save_workspace(
    State(app): State<Shared>,
    headers: HeaderMap,
    RoutePath(w): RoutePath<String>,
    Json(input): Json<WorkspaceSettingsInput>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store.save_workspace_settings(&user, &w, &input).await?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Deletion {
    /// The workspace name, typed to confirm.
    name: String,
}
async fn schedule_deletion(
    State(app): State<Shared>,
    headers: HeaderMap,
    RoutePath(w): RoutePath<String>,
    Json(input): Json<Deletion>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store
            .schedule_workspace_deletion(&user, &w, &input.name)
            .await?,
    ))
}
async fn cancel_deletion(
    State(app): State<Shared>,
    headers: HeaderMap,
    RoutePath(w): RoutePath<String>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    app.store.cancel_workspace_deletion(&user, &w).await?;
    Ok(Json(json!({"saved": true})))
}
async fn preferences(
    State(app): State<Shared>,
    headers: HeaderMap,
    RoutePath(w): RoutePath<String>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.preferences(&user, &w).await?))
}
async fn save_preferences(
    State(app): State<Shared>,
    headers: HeaderMap,
    RoutePath(w): RoutePath<String>,
    Json(input): Json<Preferences>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.save_preferences(&user, &w, &input).await?))
}
async fn report_problem(
    State(app): State<Shared>,
    headers: HeaderMap,
    RoutePath(w): RoutePath<String>,
    Json(input): Json<ProblemReport>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.report_problem(&user, &w, &input).await?))
}
/// Whether all agents are paused; every member sees it.
async fn pause_state(
    State(app): State<Shared>,
    headers: HeaderMap,
    RoutePath(w): RoutePath<String>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.agents_pause_state(&user, &w).await?))
}
/// An admin pauses all agents: everything running or waiting stops at once.
async fn pause(
    State(app): State<Shared>,
    headers: HeaderMap,
    RoutePath(w): RoutePath<String>,
    Json(input): Json<PauseInput>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.pause_agents(&user, &w, &input).await?))
}
async fn resume(
    State(app): State<Shared>,
    headers: HeaderMap,
    RoutePath(w): RoutePath<String>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.resume_agents(&user, &w).await?))
}

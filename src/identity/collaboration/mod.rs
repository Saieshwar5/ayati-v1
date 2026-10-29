//! Shared company features use the PostgreSQL workspace transaction boundary.
mod catalog;
mod files;
mod work;
use super::*;
use axum::{Json, extract::Path as RoutePath, http::HeaderMap};
pub(super) use catalog::{agents, validate_agent};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .merge(work::routes())
        .route(
            "/api/identity/workspaces/{w}/shared-agents",
            get(agent_list).post(agent_create),
        )
        .route(
            "/api/identity/workspaces/{w}/teams",
            get(team_list).post(team_save),
        )
        .route(
            "/api/identity/workspaces/{w}/chats/{chat}/members",
            post(members),
        )
        .route(
            "/api/identity/workspaces/{w}/feedback",
            get(feedback_list).post(feedback_add),
        )
        .route(
            "/api/identity/workspaces/{w}/feedback/{id}",
            post(feedback_update),
        )
        .route("/api/identity/workspaces/{w}/metrics", get(metrics))
        .route(
            "/api/identity/workspaces/{w}/chats/{chat}/objects",
            post(files::upload).layer(axum::extract::DefaultBodyLimit::max(10 * 1024 * 1024)),
        )
        .route(
            "/api/identity/workspaces/{w}/objects/{id}",
            get(files::download),
        )
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentInput {
    id: String,
    name: String,
    role: String,
    model: String,
}
async fn agent_list(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    workspaces::membership(&app, &user, &w).await?;
    Ok(Json(json!({"agents":agents(&app,&user,&w).await?})))
}
async fn agent_create(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Json(input): Json<AgentInput>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    app.store
        .save_shared_agent(&user, &w, &input.id, &input.name, &input.role, &input.model)
        .await?;
    Ok(Json(json!({"ok":true})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TeamInput {
    id: String,
    name: String,
    users: Vec<String>,
}
async fn team_list(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(json!({"teams":app.store.teams(&user,&w).await?})))
}
async fn team_save(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Json(input): Json<TeamInput>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    app.store
        .save_team(&user, &w, &input.id, &input.name, &input.users)
        .await?;
    Ok(Json(json!({"ok":true})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Members {
    users: Vec<String>,
}
async fn members(
    State(app): State<Shared>,
    RoutePath((w, chat)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<Members>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    app.store
        .channel_members(&user, &w, &chat, &input.users)
        .await?;
    Ok(Json(json!({"ok":true})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Feedback {
    pub id: String,
    pub chat: String,
    pub title: String,
    pub expected: String,
    pub actual: String,
    pub category: String,
    pub severity: String,
    pub evidence: String,
}
impl Feedback {
    pub fn validate(&self) -> Result<()> {
        check(
            uuid::Uuid::parse_str(&self.id).is_ok(),
            "Invalid feedback ID",
        )?;
        check(
            !self.title.trim().is_empty()
                && self.title.len() <= 160
                && !self.expected.trim().is_empty()
                && !self.actual.trim().is_empty(),
            "Describe the issue, expected result and actual result",
        )?;
        check(
            self.expected.len() <= 4000 && self.actual.len() <= 4000 && self.evidence.len() <= 4000,
            "Feedback is too long",
        )?;
        check(
            [
                "bug",
                "performance",
                "missing-feature",
                "usability",
                "reliability",
                "architecture",
            ]
            .contains(&self.category.as_str()),
            "Invalid feedback category",
        )?;
        check(
            ["low", "medium", "high", "critical"].contains(&self.severity.as_str()),
            "Invalid severity",
        )
    }
}
async fn feedback_list(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        json!({"feedback":app.store.feedback(&user,&w).await?}),
    ))
}
async fn feedback_add(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Json(input): Json<Feedback>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    app.store.add_feedback(&user, &w, &input).await?;
    Ok(Json(json!({"ok":true})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Status {
    status: String,
    revision: i64,
}
async fn feedback_update(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<Status>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    app.store
        .feedback_status(&user, &w, &id, &input.status, input.revision)
        .await?;
    Ok(Json(json!({"ok":true})))
}
async fn metrics(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let mut counts = app.store.company_counts(&user, &w).await?;
    counts["queue"] = app.store.queue_metrics(&user, &w).await?;
    Ok(Json(counts))
}
#[derive(Deserialize, Serialize)]
pub(crate) struct Object {
    pub id: String,
    pub name: String,
    pub mime: String,
    pub size: i64,
    pub checksum: String,
}

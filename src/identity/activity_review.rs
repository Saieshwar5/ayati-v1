//! Activity → Review (people-and-agents-first, Phase 5): measures, rules from
//! repeated decisions, goals and weekly reviews, for the people who can fix agents.
use super::*;
use axum::{
    Json, Router,
    extract::{Path as RoutePath, Query, State},
    http::HeaderMap,
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};

pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route("/api/identity/workspaces/{w}/activity/review", get(review))
        .route(
            "/api/identity/workspaces/{w}/activity/rule-suggestions",
            post(rule),
        )
        .route("/api/identity/workspaces/{w}/activity/goals", post(goal))
        .route(
            "/api/identity/workspaces/{w}/activity/goals/{id}/archive",
            post(archive),
        )
        .route(
            "/api/identity/workspaces/{w}/activity/weekly-review",
            post(weekly),
        )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewQuery {
    days: Option<i64>,
    scope: Option<String>,
}
async fn review(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Query(q): Query<ReviewQuery>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    crate::identity::postgres::activity::valid_scope(q.scope.as_deref())?;
    Ok(Json(
        app.store
            .activity_review(&user, &w, q.days.unwrap_or(30), q.scope.as_deref())
            .await?,
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleChoice {
    agent: String,
    connection: String,
    operation: String,
    accept: bool,
}
/// Yes or no to "let the agent do this without asking".
async fn rule(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Json(input): Json<RuleChoice>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    if input.accept {
        app.store
            .accept_rule_suggestion(&user, &w, &input.agent, &input.connection, &input.operation)
            .await?;
    } else {
        app.store
            .dismiss_rule_suggestion(&user, &w, &input.agent, &input.connection, &input.operation)
            .await?;
    }
    Ok(Json(json!({"accepted": input.accept})))
}
async fn goal(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Json(input): Json<crate::identity::postgres::activity::Goal>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.save_goal(&user, &w, &input).await?))
}
async fn archive(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    app.store.archive_goal(&user, &w, &id).await?;
    Ok(Json(json!({"archived": true})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Weekly {
    agent: String,
    enabled: bool,
}
async fn weekly(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Json(input): Json<Weekly>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store
            .set_weekly_review(&user, &w, &input.agent, input.enabled)
            .await?,
    ))
}

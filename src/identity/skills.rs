//! Skills (D234): the library, versions and an agent's skills. The browser
//! unzips uploads; the server checks every file and never runs scripts.
use super::*;
use crate::identity::postgres::skills::{
    NewSkill, NewVersion,
    files::{SkillFile, check},
};
use axum::{
    Json,
    extract::{DefaultBodyLimit, Path as RoutePath, Query},
    http::HeaderMap,
};
use serde::Deserialize;
use serde_json::{Value, json};

/// A 5 MB skill, written as JSON text.
const UPLOAD_LIMIT: usize = 8 * 1024 * 1024;

pub(super) fn routes() -> Router<Shared> {
    let base = "/api/identity/workspaces/{w}";
    Router::new()
        .route(
            &format!("{base}/skills"),
            get(list)
                .post(create)
                .layer(DefaultBodyLimit::max(UPLOAD_LIMIT)),
        )
        .route(
            &format!("{base}/skills/check"),
            post(review).layer(DefaultBodyLimit::max(UPLOAD_LIMIT)),
        )
        .route(&format!("{base}/skills/{{id}}"), get(detail))
        .route(
            &format!("{base}/skills/{{id}}/versions"),
            post(new_version).layer(DefaultBodyLimit::max(UPLOAD_LIMIT)),
        )
        .route(&format!("{base}/skills/{{id}}/share"), post(share))
        .route(&format!("{base}/skills/{{id}}/delete"), post(delete))
        .route(
            &format!("{base}/agent-profiles/{{agent}}/skills"),
            get(agent_skills).post(assign),
        )
}

async fn list(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.skills(&user, &w).await?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Files {
    files: Vec<SkillFile>,
}
/// Checks files without saving, so the person can review them first.
async fn review(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Json(input): Json<Files>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    app.store.role(&w, &user.id).await?;
    Ok(Json(json!(check(&input.files)?)))
}

async fn create(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Json(input): Json<NewSkill>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.create_skill(&user, &w, &input).await?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Shown {
    version: Option<i32>,
}
async fn detail(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Query(shown): Query<Shown>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.skill(&user, &w, &id, shown.version).await?))
}

async fn new_version(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<NewVersion>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.update_skill(&user, &w, &id, &input).await?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Sharing {
    shared: bool,
}
async fn share(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<Sharing>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    app.store.share_skill(&user, &w, &id, input.shared).await?;
    Ok(Json(json!({"saved": true})))
}

async fn delete(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    app.store.delete_skill(&user, &w, &id).await?;
    Ok(Json(json!({"saved": true})))
}

async fn agent_skills(
    State(app): State<Shared>,
    RoutePath((w, agent)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.agent_skills(&user, &w, &agent).await?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Assign {
    skill: String,
    on: bool,
}
async fn assign(
    State(app): State<Shared>,
    RoutePath((w, agent)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<Assign>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store
            .assign_skill(&user, &w, &agent, &input.skill, input.on)
            .await?,
    ))
}

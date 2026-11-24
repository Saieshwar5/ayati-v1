//! Memory (schema 142): the workspace's memory for admins (Settings › Memory),
//! an agent's job and room notes for its owners and admins, and what an agent
//! remembers about you, for you alone. See runtime/memory.md.
use super::*;
use crate::identity::postgres::memory::changes::{NoteInput, TopicInput};
use crate::identity::postgres::memory::manage::Page;
use std::collections::HashMap;

pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route(
            "/api/identity/workspaces/{w}/memory",
            get(workspace_page).post(workspace_note),
        )
        .route(
            "/api/identity/workspaces/{w}/memory/topics",
            post(workspace_topic),
        )
        .route(
            "/api/identity/workspaces/{w}/memory/notes/{note}/{change}",
            post(workspace_change),
        )
        .route(
            "/api/identity/workspaces/{w}/memory/changes/{change}/undo",
            post(workspace_undo),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{id}/memory",
            get(agent_page).post(agent_note),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{id}/memory/topics",
            post(agent_topic),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{id}/memory/notes/{note}/{change}",
            post(agent_change),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{id}/memory/changes/{change}/undo",
            post(agent_undo),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{id}/memory/about-me",
            get(about_me),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{id}/memory/about-me/{note}/remove",
            post(forget_me),
        )
}

type Query = axum::extract::Query<HashMap<String, String>>;

async fn page(
    app: &Shared,
    headers: &HeaderMap,
    w: &str,
    page: Page<'_>,
    query: &HashMap<String, String>,
) -> ApiResult<Json<Value>> {
    let user = identity(app, headers).await?;
    let view = query.get("view").map_or("topics", String::as_str);
    let mut result = app
        .store
        .memory_page(
            &user,
            w,
            page,
            view,
            query.get("q").map(String::as_str),
            query.get("topic").map(String::as_str),
        )
        .await?;
    result["summaries"] = json!(app.config.learning_model.is_some());
    Ok(Json(result))
}

async fn workspace_page(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    query: Query,
) -> ApiResult<Json<Value>> {
    page(&app, &headers, &w, Page::Workspace, &query).await
}

async fn agent_page(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
    query: Query,
) -> ApiResult<Json<Value>> {
    page(&app, &headers, &w, Page::Agent(&id), &query).await
}

async fn workspace_note(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Json(input): Json<NoteInput>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    app.store
        .save_memory_note(&user, &w, Page::Workspace, &input)
        .await?;
    Ok(Json(json!({"ok":true})))
}

async fn agent_note(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<NoteInput>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    app.store
        .save_memory_note(&user, &w, Page::Agent(&id), &input)
        .await?;
    Ok(Json(json!({"ok":true})))
}

async fn workspace_topic(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Json(input): Json<TopicInput>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    app.store
        .save_memory_topic(&user, &w, Page::Workspace, &input)
        .await?;
    Ok(Json(json!({"ok":true})))
}

async fn agent_topic(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<TopicInput>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    app.store
        .save_memory_topic(&user, &w, Page::Agent(&id), &input)
        .await?;
    Ok(Json(json!({"ok":true})))
}

async fn workspace_change(
    State(app): State<Shared>,
    RoutePath((w, note, change)): RoutePath<(String, String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    app.store
        .change_memory_note(&user, &w, Page::Workspace, &note, &change)
        .await?;
    Ok(Json(json!({"ok":true})))
}

async fn agent_change(
    State(app): State<Shared>,
    RoutePath((w, id, note, change)): RoutePath<(String, String, String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    app.store
        .change_memory_note(&user, &w, Page::Agent(&id), &note, &change)
        .await?;
    Ok(Json(json!({"ok":true})))
}

async fn workspace_undo(
    State(app): State<Shared>,
    RoutePath((w, change)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    app.store
        .undo_memory_change(&user, &w, Page::Workspace, &change)
        .await?;
    Ok(Json(json!({"ok":true})))
}

async fn agent_undo(
    State(app): State<Shared>,
    RoutePath((w, id, change)): RoutePath<(String, String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    app.store
        .undo_memory_change(&user, &w, Page::Agent(&id), &change)
        .await?;
    Ok(Json(json!({"ok":true})))
}

async fn about_me(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.memory_about_me(&user, &w, &id).await?))
}

async fn forget_me(
    State(app): State<Shared>,
    RoutePath((w, id, note)): RoutePath<(String, String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    app.store.forget_about_me(&user, &w, &id, &note).await?;
    Ok(Json(json!({"ok":true})))
}

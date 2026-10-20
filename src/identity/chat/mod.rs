mod runner;
mod store;
#[cfg(test)]
mod tests;
use super::*;
use axum::{Json, extract::Path as RoutePath};
use serde::Deserialize;
use serde_json::{Value, json};
pub(super) use store::schema;

pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route(
            "/api/identity/workspaces/{workspace}/chat-directory",
            get(directory),
        )
        .route(
            "/api/identity/workspaces/{workspace}/chat-grants",
            post(grant),
        )
        .route(
            "/api/identity/workspaces/{workspace}/chats",
            get(list).post(create),
        )
        .route(
            "/api/identity/workspaces/{workspace}/chats/{chat}/messages",
            get(messages).post(send),
        )
}
async fn directory(
    State(app): State<Shared>,
    RoutePath(workspace): RoutePath<String>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let role = workspaces::membership(&app, &user, &workspace)?;
    let state = app.runtimes.get(&workspace, port(&app)).await?;
    let profiles = state
        .workspace
        .context("Workspace unavailable")?
        .profiles()?;
    let db = app.store.0.lock().unwrap();
    let mut agents = Vec::new();
    for profile in profiles.into_iter().filter(|p| p.state == "ready") {
        if store::agent_access(&db, &user.id, &workspace, &profile.id)? {
            agents.push(json!({"id":profile.id,"name":profile.name}));
        }
    }
    let users = db
        .prepare(
            "SELECT u.id,u.email,m.role FROM users u JOIN members m ON m.user=u.id WHERE m.workspace=?1",
        )?
        .query_map([&workspace], |r| {
            Ok(json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"role":r.get::<_,String>(2)?}))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let grants = if role == "Owner" {
        db.prepare("SELECT user,agent FROM chat_access WHERE workspace=?1")?
            .query_map([&workspace], |r| {
                Ok(json!({"user":r.get::<_,String>(0)?,"agent":r.get::<_,String>(1)?}))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
    } else {
        vec![]
    };
    Ok(Json(
        json!({"users":users,"agents":agents,"grants":grants,"role":role}),
    ))
}
fn port(app: &App) -> u16 {
    app.origin
        .rsplit(':')
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8765)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Grant {
    user: String,
    agent: String,
    allowed: bool,
}
async fn grant(
    State(app): State<Shared>,
    RoutePath(workspace): RoutePath<String>,
    headers: axum::http::HeaderMap,
    Json(input): Json<Grant>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    check(
        workspaces::membership(&app, &user, &workspace)? == "Owner",
        "Only the owner can share agents",
    )?;
    let state = app.runtimes.get(&workspace, port(&app)).await?;
    state
        .workspace
        .context("Workspace unavailable")?
        .agent(&input.agent)?;
    app.store
        .chat_grant(&user, &workspace, &input.user, &input.agent, input.allowed)?;
    Ok(Json(json!({"ok":true})))
}
async fn list(
    State(app): State<Shared>,
    RoutePath(workspace): RoutePath<String>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.chats(&user, &workspace)?))
}
async fn create(
    State(app): State<Shared>,
    RoutePath(workspace): RoutePath<String>,
    headers: axum::http::HeaderMap,
    Json(input): Json<store::NewChat>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    workspaces::membership(&app, &user, &workspace)?;
    if !input.agents.is_empty() {
        let state = app.runtimes.get(&workspace, port(&app)).await?;
        let registry = state.workspace.context("Workspace unavailable")?;
        for id in &input.agents {
            registry.agent(id)?;
        }
    }
    Ok(Json(
        json!({"id":app.store.chat_create(&user,&workspace,&input)?}),
    ))
}
async fn messages(
    State(app): State<Shared>,
    RoutePath((workspace, chat)): RoutePath<(String, String)>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.chat_messages(&user, &workspace, &chat)?))
}
async fn send(
    State(app): State<Shared>,
    RoutePath((workspace, chat)): RoutePath<(String, String)>,
    headers: axum::http::HeaderMap,
    Json(input): Json<store::NewMessage>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    // Validate the conversation before loading any agent runtime.
    app.store.chat_messages(&user, &workspace, &chat)?;
    if let Some(id) = &input.agent {
        app.runtimes
            .get(&workspace, port(&app))
            .await?
            .workspace
            .context("Workspace unavailable")?
            .agent(id)?;
    }
    if let Some(job) = app.store.chat_send(&user, &workspace, &chat, &input)? {
        tokio::spawn(runner::run(app.clone(), job));
    }
    Ok(Json(json!({"ok":true})))
}

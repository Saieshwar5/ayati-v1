mod runner;
pub(crate) mod store;
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
    let _role = workspaces::membership(&app, &user, &workspace).await?;
    let state = app.runtimes.get(&workspace, port(&app)).await?;
    let profiles = state
        .workspace
        .context("Workspace unavailable")?
        .profiles()?;
    let mut agents = Vec::new();
    for profile in profiles.into_iter().filter(|p| p.state == "ready") {
        if app
            .store
            .agent_access(&user.id, &workspace, &profile.id)
            .await?
        {
            agents.push(json!({"id":profile.id,"name":profile.name}));
        }
    }
    let mut directory = app.store.directory(&user, &workspace).await?;
    directory["agents"] = json!(agents);
    Ok(Json(directory))
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
        workspaces::membership(&app, &user, &workspace).await? == "Owner",
        "Only the owner can share agents",
    )?;
    let state = app.runtimes.get(&workspace, port(&app)).await?;
    state
        .workspace
        .context("Workspace unavailable")?
        .agent(&input.agent)?;
    app.store
        .chat_grant(&user, &workspace, &input.user, &input.agent, input.allowed)
        .await?;
    Ok(Json(json!({"ok":true})))
}
async fn list(
    State(app): State<Shared>,
    RoutePath(workspace): RoutePath<String>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.chats(&user, &workspace).await?))
}
async fn create(
    State(app): State<Shared>,
    RoutePath(workspace): RoutePath<String>,
    headers: axum::http::HeaderMap,
    Json(input): Json<store::NewChat>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    workspaces::membership(&app, &user, &workspace).await?;
    if !input.agents.is_empty() {
        let state = app.runtimes.get(&workspace, port(&app)).await?;
        let registry = state.workspace.context("Workspace unavailable")?;
        for id in &input.agents {
            registry.agent(id)?;
        }
    }
    Ok(Json(
        json!({"id":app.store.chat_create(&user,&workspace,&input).await?}),
    ))
}
async fn messages(
    State(app): State<Shared>,
    RoutePath((workspace, chat)): RoutePath<(String, String)>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store.chat_messages(&user, &workspace, &chat).await?,
    ))
}
async fn send(
    State(app): State<Shared>,
    RoutePath((workspace, chat)): RoutePath<(String, String)>,
    headers: axum::http::HeaderMap,
    Json(input): Json<store::NewMessage>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    // Validate the conversation before loading any agent runtime.
    app.store.chat_messages(&user, &workspace, &chat).await?;
    if let Some(id) = &input.agent {
        app.runtimes
            .get(&workspace, port(&app))
            .await?
            .workspace
            .context("Workspace unavailable")?
            .agent(id)?;
    }
    if let Some(job) = app
        .store
        .chat_send(&user, &workspace, &chat, &input)
        .await?
        && matches!(app.store, super::database::Database::Sqlite(_))
    {
        tokio::spawn(runner::run(app.clone(), job));
    }
    Ok(Json(json!({"ok":true})))
}

/// Poll durable PostgreSQL jobs. PostgreSQL claims fence stale worker publication.
pub(super) fn start_dispatcher(app: Shared) {
    if !matches!(app.store, super::database::Database::Postgres(_)) {
        return;
    }
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = app.runtimes.stopping.cancelled() => break,
                _ = tokio::time::sleep(std::time::Duration::from_millis(500)) => {}
            }
            if app.runtimes.chat_slots.available_permits() == 0 {
                continue;
            }
            if let super::database::Database::Postgres(pg) = &app.store {
                match pg.claim_job().await {
                    Ok(Some(job)) => {
                        tokio::spawn(runner::run(app.clone(), job));
                    }
                    Ok(None) => {}
                    Err(_) => {
                        tracing::warn!("Shared work database unavailable; dispatch will retry")
                    }
                }
            }
        }
    });
}

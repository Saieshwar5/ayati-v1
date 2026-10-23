pub(super) mod execution;
pub(crate) mod mentions;
mod runner;
pub(crate) mod store;
mod stream_limit;
mod sync;
#[cfg(test)]
mod tests;
use super::*;
use axum::{Json, extract::Path as RoutePath};
use serde::Deserialize;
use serde_json::{Value, json};
pub(super) use store::schema;

pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .merge(sync::routes())
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
        .route(
            "/api/identity/workspaces/{workspace}/chats/{chat}/background-requests",
            post(background),
        )
        .route(
            "/api/identity/workspaces/{workspace}/chats/{chat}/messages/{reply}/cancel",
            post(cancel),
        )
}
async fn cancel(
    State(app): State<Shared>,
    RoutePath((workspace, chat, reply)): RoutePath<(String, String, String)>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let pg = super::collaboration::pg(&app)?;
    pg.cancel_assignment(&user, &workspace, &chat, &reply)
        .await?;
    Ok(Json(json!({"ok":true})))
}
async fn directory(
    State(app): State<Shared>,
    RoutePath(workspace): RoutePath<String>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let _role = workspaces::membership(&app, &user, &workspace).await?;
    let agents = super::collaboration::agents(&app, &user, &workspace).await?;
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
    super::collaboration::validate_agent(&app, &user, &workspace, &input.agent).await?;
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
    for id in &input.agents {
        super::collaboration::validate_agent(&app, &user, &workspace, id).await?;
    }
    Ok(Json(
        json!({"id":app.store.chat_create(&user,&workspace,&input).await?}),
    ))
}
#[derive(Deserialize)]
struct Page {
    before: Option<i64>,
}
async fn messages(
    State(app): State<Shared>,
    RoutePath((workspace, chat)): RoutePath<(String, String)>,
    headers: axum::http::HeaderMap,
    axum::extract::Query(page): axum::extract::Query<Page>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    if let super::database::Database::Postgres(pg) = &app.store {
        return Ok(Json(
            pg.chat_page(&user, &workspace, &chat, page.before).await?,
        ));
    }
    check(page.before.is_none(), "Older history requires PostgreSQL")?;
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
    if let super::database::Database::Postgres(pg) = &app.store {
        pg.check_chat(&user, &workspace, &chat).await?;
    } else {
        app.store.chat_messages(&user, &workspace, &chat).await?;
    }
    if let Some(id) = &input.agent {
        super::collaboration::validate_agent(&app, &user, &workspace, id).await?;
    }
    if let Some(job) = app
        .store
        .chat_send(&user, &workspace, &chat, &input)
        .await?
    {
        if matches!(app.store, super::database::Database::Sqlite(_)) {
            tokio::spawn(runner::run(app.clone(), job));
        } else {
            app.runtimes.chat_wake.notify_one();
        }
    }
    if let super::database::Database::Postgres(pg) = &app.store {
        return Ok(Json(
            json!({"ok":true,"message":pg.accepted_message(&user,&workspace,&chat,&input.id).await.ok()}),
        ));
    }
    Ok(Json(json!({"ok":true})))
}

async fn background(
    State(app): State<Shared>,
    RoutePath((workspace, chat)): RoutePath<(String, String)>,
    headers: axum::http::HeaderMap,
    Json(input): Json<store::NewMessage>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let pg = super::collaboration::pg(&app)?;
    pg.check_chat(&user, &workspace, &chat).await?;
    let agent = input
        .agent
        .as_ref()
        .context("Background requests need an agent")?;
    super::collaboration::validate_agent(&app, &user, &workspace, agent).await?;
    if pg
        .chat_background(&user, &workspace, &chat, &input)
        .await?
        .is_some()
    {
        app.runtimes.chat_wake.notify_one();
    }
    Ok(Json(
        json!({"ok":true,"work_class":"background","message":pg.accepted_message(&user,&workspace,&chat,&input.id).await.ok()}),
    ))
}

/// Poll durable PostgreSQL jobs. PostgreSQL claims fence stale worker publication.
pub(super) fn start_dispatcher(app: Shared) -> Result<()> {
    let embedded = super::worker::embedded_mode(
        std::env::var("AYATI_CHAT_EXECUTION").ok().as_deref(),
        matches!(app.store, super::database::Database::Postgres(_)),
    )?;
    sync::start(app.clone());
    if !embedded || !matches!(app.store, super::database::Database::Postgres(_)) {
        return Ok(());
    }
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = app.runtimes.stopping.cancelled() => break,
                _ = app.runtimes.chat_wake.notified() => {},
                _ = tokio::time::sleep(std::time::Duration::from_millis(500)) => {}
            }
            if app
                .simulation
                .as_ref()
                .is_some_and(|sim| sim.paused.load(std::sync::atomic::Ordering::Acquire))
            {
                continue;
            }
            if let super::database::Database::Postgres(pg) = &app.store {
                // Reserve capacity before claiming: never lease jobs waiting for a local slot.
                while let Ok(slot) = app.runtimes.chat_slots.clone().try_acquire_owned() {
                    match pg.claim_job().await {
                        Ok(Some(job)) => {
                            tokio::spawn(runner::run_claimed(app.clone(), job, slot));
                        }
                        Ok(None) => break,
                        Err(_) => {
                            tracing::warn!("Shared work database unavailable; dispatch will retry");
                            break;
                        }
                    }
                }
            }
        }
    });
    Ok(())
}

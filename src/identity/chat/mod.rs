pub(super) mod execution;
pub(crate) mod mentions;
mod message_actions;
pub(crate) mod model;
mod notifications;
mod organization;
pub(crate) mod organization_model;
mod stream_limit;
mod sync;
mod task_progress;
use super::*;
use axum::{Json, extract::Path as RoutePath};
use serde::Deserialize;
use serde_json::{Value, json};

pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .merge(sync::routes())
        .merge(task_progress::routes())
        .merge(organization::routes())
        .merge(message_actions::routes())
        .merge(notifications::routes())
        .route(
            "/api/identity/workspaces/{workspace}/chats/{chat}/messages/{message}",
            get(focus),
        )
        .route(
            "/api/identity/workspaces/{workspace}/chats/{chat}/messages/{message}/response",
            get(response),
        )
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
async fn focus(
    State(app): State<Shared>,
    RoutePath((w, chat, message)): RoutePath<(String, String, String)>,
    headers: axum::http::HeaderMap,
    axum::extract::Query(page): axum::extract::Query<Page>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let pg = &app.store;
    let mut data = pg.chat_focus(&user, &w, &chat, &message).await?;
    if page.compact {
        crate::response::compact_history(&mut data);
    }
    Ok(Json(data))
}
async fn response(
    State(app): State<Shared>,
    RoutePath((w, chat, message)): RoutePath<(String, String, String)>,
    headers: axum::http::HeaderMap,
    axum::extract::Query(input): axum::extract::Query<crate::response::Read>,
) -> ApiResult<Response> {
    let user = identity(&app, &headers).await?;
    let data = app
        .store
        .response_body(&user, &w, &chat, &message, input.metadata)
        .await?;
    Ok(crate::response::deliver(&message, data, &input))
}
async fn cancel(
    State(app): State<Shared>,
    RoutePath((workspace, chat, reply)): RoutePath<(String, String, String)>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let pg = &app.store;
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
        workspaces::membership(&app, &user, &workspace).await? == "Admin",
        "Only admins can share agents",
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
    Ok(Json(organization::chats(&app, &user, &workspace).await?))
}
async fn create(
    State(app): State<Shared>,
    RoutePath(workspace): RoutePath<String>,
    headers: axum::http::HeaderMap,
    Json(input): Json<model::NewChat>,
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
    #[serde(default)]
    compact: bool,
}
async fn messages(
    State(app): State<Shared>,
    RoutePath((workspace, chat)): RoutePath<(String, String)>,
    headers: axum::http::HeaderMap,
    axum::extract::Query(page): axum::extract::Query<Page>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let mut data = app
        .store
        .chat_page(&user, &workspace, &chat, page.before)
        .await?;
    if page.compact {
        crate::response::compact_history(&mut data);
    }
    Ok(Json(data))
}
async fn send(
    State(app): State<Shared>,
    RoutePath((workspace, chat)): RoutePath<(String, String)>,
    headers: axum::http::HeaderMap,
    Json(input): Json<model::NewMessage>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    // Validate the conversation before loading any agent runtime.
    app.store.check_chat(&user, &workspace, &chat).await?;
    if let Some(id) = &input.agent {
        super::collaboration::validate_agent(&app, &user, &workspace, id).await?;
    }
    if app
        .store
        .chat_send(&user, &workspace, &chat, &input)
        .await?
        .is_some()
    {
        app.runtimes.chat_wake.notify_one();
    }
    Ok(Json(
        json!({"ok":true,"message":app.store.accepted_message(&user,&workspace,&chat,&input.id).await.ok()}),
    ))
}

async fn background(
    State(app): State<Shared>,
    RoutePath((workspace, chat)): RoutePath<(String, String)>,
    headers: axum::http::HeaderMap,
    Json(input): Json<model::NewMessage>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let pg = &app.store;
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

/// Start chat change sync. Pooled executors receive agent replies through worker
/// control; only the scripted company simulation answers inside the controller.
pub(super) fn start_dispatcher(app: Shared) {
    sync::start(app.clone());
    if app.simulation.is_some() && !app.store.pooled {
        super::simulation::dispatch::start(app);
    }
}

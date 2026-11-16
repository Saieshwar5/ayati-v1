//! Routes to edit or delete your own message and to react to messages.
use super::*;

pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route(
            "/api/identity/workspaces/{w}/chats/{chat}/messages/{message}/edit",
            post(edit),
        )
        .route(
            "/api/identity/workspaces/{w}/chats/{chat}/messages/{message}/delete",
            post(delete),
        )
        .route(
            "/api/identity/workspaces/{w}/chats/{chat}/messages/{message}/reactions",
            post(react),
        )
        .route(
            "/api/identity/workspaces/{w}/chats/{chat}/messages/{message}/forward",
            post(forward),
        )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Edit {
    content: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reaction {
    emoji: String,
    on: bool,
}

async fn edit(
    State(app): State<Shared>,
    RoutePath((w, chat, message)): RoutePath<(String, String, String)>,
    headers: axum::http::HeaderMap,
    Json(input): Json<Edit>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    app.store
        .edit_message(&user, &w, &chat, &message, &input.content)
        .await?;
    Ok(Json(json!({"ok":true})))
}

async fn delete(
    State(app): State<Shared>,
    RoutePath((w, chat, message)): RoutePath<(String, String, String)>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let stopped = app.store.delete_message(&user, &w, &chat, &message).await?;
    Ok(Json(json!({"ok":true,"stopped_requests":stopped})))
}

/// Forward a message to a person or an agent, in your direct chat with them
/// (schema 125). Sending it to an agent asks that agent.
async fn forward(
    State(app): State<Shared>,
    RoutePath((w, chat, message)): RoutePath<(String, String, String)>,
    headers: axum::http::HeaderMap,
    Json(input): Json<model::Forward>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    if input.to.kind == "agent" {
        super::super::collaboration::validate_agent(&app, &user, &w, &input.to.id).await?;
    }
    let (target, job) = app
        .store
        .forward_message(&user, &w, &chat, &message, &input)
        .await?;
    if job.is_some() {
        app.runtimes.chat_wake.notify_one();
    }
    Ok(Json(json!({"ok":true,"chat":target})))
}

async fn react(
    State(app): State<Shared>,
    RoutePath((w, chat, message)): RoutePath<(String, String, String)>,
    headers: axum::http::HeaderMap,
    Json(input): Json<Reaction>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    app.store
        .react(&user, &w, &chat, &message, &input.emoji, input.on)
        .await?;
    Ok(Json(json!({"ok":true})))
}

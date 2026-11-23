//! Routes for a member's assistant threads (schema 128): list them, write a
//! new one, start a suggested one, and stop, resume or delete one; and set
//! what the assistant does in a chat (schema 141).
use super::*;
use crate::identity::postgres::assistants::threads::{NewThread, modes::SetReplies};

pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route(
            "/api/identity/workspaces/{w}/assistant/threads",
            get(list).post(create),
        )
        .route(
            "/api/identity/workspaces/{w}/assistant/replies",
            post(replies),
        )
        .route(
            "/api/identity/workspaces/{w}/assistant/threads/{thread}/draft/discard",
            post(discard),
        )
        .route(
            "/api/identity/workspaces/{w}/assistant/threads/{thread}/state",
            post(set),
        )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SetState {
    /// `running` (start or resume), `stopped` or `deleted`.
    state: String,
}

async fn list(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.threads(&user, &w).await?))
}

async fn create(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: axum::http::HeaderMap,
    Json(input): Json<NewThread>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    if input.kind == "agent" {
        super::super::collaboration::validate_agent(&app, &user, &w, &input.counterpart).await?;
    }
    let (chat, job) = app.store.thread_create(&user, &w, &input).await?;
    if job.is_some() {
        app.runtimes.chat_wake.notify_one();
    }
    Ok(Json(json!({"ok":true,"chat":chat})))
}

/// What the member's assistant does in a direct chat: nothing, write each reply
/// for them to send, or reply for them (schema 141).
async fn replies(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: axum::http::HeaderMap,
    Json(input): Json<SetReplies>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let thread = app.store.thread_position(&user, &w, &input).await?;
    // What the other side already wrote is handed to the assistant now.
    app.store.work.schedule.notify_one();
    Ok(Json(json!({"ok":true,"thread":thread})))
}

/// The member does not want the reply that waits for them in a thread.
async fn discard(
    State(app): State<Shared>,
    RoutePath((w, thread)): RoutePath<(String, String)>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    app.store.thread_draft_discard(&user, &w, &thread).await?;
    Ok(Json(json!({"ok":true})))
}

async fn set(
    State(app): State<Shared>,
    RoutePath((w, thread)): RoutePath<(String, String)>,
    headers: axum::http::HeaderMap,
    Json(input): Json<SetState>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let job = app
        .store
        .thread_set(&user, &w, &thread, &input.state)
        .await?;
    if job.is_some() {
        app.runtimes.chat_wake.notify_one();
    }
    // An answer that came while it was stopped goes to the assistant now.
    app.store.work.schedule.notify_one();
    Ok(Json(json!({"ok":true})))
}

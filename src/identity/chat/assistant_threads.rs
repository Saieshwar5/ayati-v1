//! Routes for a member's assistant threads (schema 128): list them, write a
//! new one, start a suggested one, and stop, resume or delete one.
use super::*;
use crate::identity::postgres::assistants::threads::{NewThread, takeover::TakeOver};

pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route(
            "/api/identity/workspaces/{w}/assistant/threads",
            get(list).post(create),
        )
        .route(
            "/api/identity/workspaces/{w}/assistant/take-over",
            post(take_over),
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

/// Hand a direct chat over from one of the other side's messages (schema 136).
async fn take_over(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: axum::http::HeaderMap,
    Json(input): Json<TakeOver>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let chat = app.store.thread_take_over(&user, &w, &input).await?;
    // The assistant is handed the chosen message now.
    app.store.work.schedule.notify_one();
    Ok(Json(json!({"ok":true,"chat":chat})))
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

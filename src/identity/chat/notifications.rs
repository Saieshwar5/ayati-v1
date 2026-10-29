//! Routes for personal notification levels per conversation.
use super::*;

pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route(
            "/api/identity/workspaces/{w}/chat-notifications",
            get(levels),
        )
        .route(
            "/api/identity/workspaces/{w}/chats/{chat}/notifications",
            post(set_level),
        )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Level {
    level: String,
}

async fn levels(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.chat_notify_levels(&user, &w).await?))
}

async fn set_level(
    State(app): State<Shared>,
    RoutePath((w, chat)): RoutePath<(String, String)>,
    headers: axum::http::HeaderMap,
    Json(input): Json<Level>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    app.store
        .set_chat_notify(&user, &w, &chat, &input.level)
        .await?;
    Ok(Json(json!({"ok":true,"level":input.level})))
}

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
    let deleted = app.store.delete_message(&user, &w, &chat, &message).await?;
    for file in &deleted.files {
        crate::identity::collaboration::remove_stored_object(&app, &w, file).await;
    }
    Ok(Json(json!({"ok":true,"stopped_requests":deleted.stopped})))
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

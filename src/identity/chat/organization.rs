use super::*;
use organization_model::{Lifecycle, SavePins};
pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route(
            "/api/identity/workspaces/{w}/chat-pins",
            get(pins).post(save_pins),
        )
        .route(
            "/api/identity/workspaces/{w}/chats/{chat}/lifecycle",
            get(lifecycle).post(change_lifecycle),
        )
}
pub(super) async fn chats(app: &App, user: &User, w: &str) -> ApiResult<Value> {
    Ok(app.store.organized_chats(user, w).await?)
}
pub(super) async fn read_pins(app: &App, user: &User, w: &str) -> ApiResult<Value> {
    Ok(app.store.chat_pins(user, w).await?)
}
async fn pins(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    workspaces::membership(&app, &user, &w).await?;
    Ok(Json(read_pins(&app, &user, &w).await?))
}
async fn save_pins(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: axum::http::HeaderMap,
    Json(input): Json<SavePins>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let pg = &app.store;
    Ok(Json(pg.save_chat_pins(&user, &w, &input).await?))
}
async fn lifecycle(
    State(app): State<Shared>,
    RoutePath((w, chat)): RoutePath<(String, String)>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store.channel_lifecycle(&user, &w, &chat, None).await?,
    ))
}
async fn change_lifecycle(
    State(app): State<Shared>,
    RoutePath((w, chat)): RoutePath<(String, String)>,
    headers: axum::http::HeaderMap,
    Json(input): Json<Lifecycle>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store
            .channel_lifecycle(&user, &w, &chat, Some(&input))
            .await?,
    ))
}

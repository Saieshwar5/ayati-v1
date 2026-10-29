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
    if let super::super::database::Database::Postgres(pg) = &app.store {
        return Ok(pg.organized_chats(user, w).await?);
    }
    let mut data = app.store.chats(user, w).await?;
    data["preferences"] = json!({"available":false,"revision":0,"pins":[]});
    Ok(data)
}
pub(super) async fn read_pins(app: &App, user: &User, w: &str) -> ApiResult<Value> {
    let super::super::database::Database::Postgres(pg) = &app.store else {
        return Ok(json!({"available":false,"revision":0,"pins":[]}));
    };
    Ok(pg.chat_pins(user, w).await?)
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
    let pg = super::super::collaboration::pg(&app)?;
    Ok(Json(pg.save_chat_pins(&user, &w, &input).await?))
}
async fn lifecycle(
    State(app): State<Shared>,
    RoutePath((w, chat)): RoutePath<(String, String)>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        super::super::collaboration::pg(&app)?
            .channel_lifecycle(&user, &w, &chat, None)
            .await?,
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
        super::super::collaboration::pg(&app)?
            .channel_lifecycle(&user, &w, &chat, Some(&input))
            .await?,
    ))
}

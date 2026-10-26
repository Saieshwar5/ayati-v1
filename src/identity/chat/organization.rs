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
async fn locals(app: &App, user: &User, w: &str) -> ApiResult<Vec<String>> {
    if workspaces::membership(app, user, w).await? != "Owner"
        || app.simulation.is_some()
        || super::super::hosting::hosted(app)
    {
        return Ok(vec![]);
    }
    Ok(super::super::collaboration::agents(app, user, w)
        .await?
        .into_iter()
        .filter(|agent| agent["shared"] != true)
        .filter_map(|agent| agent["id"].as_str().map(str::to_owned))
        .collect())
}
pub(super) async fn chats(app: &App, user: &User, w: &str) -> ApiResult<Value> {
    if let super::super::database::Database::Postgres(pg) = &app.store {
        // Resolve runtime metadata before the database snapshot, so catalog I/O
        // cannot delay release of already-read conversation data or hold its locks.
        let local = locals(app, user, w).await?;
        return Ok(pg.organized_chats(user, w, &local).await?);
    }
    let mut data = app.store.chats(user, w).await?;
    data["preferences"] = json!({"available":false,"revision":0,"pins":[]});
    Ok(data)
}
pub(super) async fn read_pins(app: &App, user: &User, w: &str) -> ApiResult<Value> {
    let super::super::database::Database::Postgres(pg) = &app.store else {
        return Ok(json!({"available":false,"revision":0,"pins":[]}));
    };
    Ok(pg.chat_pins(user, w, &locals(app, user, w).await?).await?)
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
    Ok(Json(
        pg.save_chat_pins(&user, &w, &locals(&app, &user, &w).await?, &input)
            .await?,
    ))
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

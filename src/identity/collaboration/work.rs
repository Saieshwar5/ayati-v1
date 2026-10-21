use super::*;
pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route("/api/identity/workspaces/{w}/work", get(list))
        .route("/api/identity/workspaces/{w}/work/{id}", post(update))
        .route(
            "/api/identity/workspaces/{w}/chats/{chat}/work-grants",
            post(grant),
        )
        .route(
            "/api/identity/workspaces/{w}/chats/{chat}/settings",
            post(settings),
        )
}
async fn list(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        json!({"work":pg(&app)?.company_work(&user,&w).await?}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Update {
    status: String,
    assignee: String,
    revision: i64,
}
async fn update(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<Update>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    pg(&app)?
        .work_update(
            &user,
            &w,
            &id,
            &input.status,
            &input.assignee,
            input.revision,
        )
        .await?;
    Ok(Json(json!({"ok":true})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Grant {
    agent: String,
    allowed: bool,
}
async fn grant(
    State(app): State<Shared>,
    RoutePath((w, chat)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<Grant>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    pg(&app)?
        .work_grant(&user, &w, &chat, &input.agent, input.allowed)
        .await?;
    Ok(Json(json!({"ok":true})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    #[serde(default)]
    name: Option<String>,
    visibility: String,
    announcements: bool,
}
async fn settings(
    State(app): State<Shared>,
    RoutePath((w, chat)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<Settings>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    pg(&app)?
        .channel_settings(
            &user,
            &w,
            &chat,
            &input.visibility,
            input.announcements,
            input.name.as_deref(),
        )
        .await?;
    Ok(Json(json!({"ok":true})))
}

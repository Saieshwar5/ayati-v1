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
        json!({"work":app.store.company_work(&user,&w).await?}),
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
    app.store
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
    app.store
        .work_grant(&user, &w, &chat, &input.agent, input.allowed)
        .await?;
    Ok(Json(json!({"ok":true})))
}
/// Channel settings saved together from Details. The creator and workspace
/// admins may change the name, description and who can post; only admins may
/// change who can find it, because that changes who can read its history.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ChannelSettings {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    pub visibility: String,
    pub announcements: bool,
}
impl ChannelSettings {
    pub fn validate(&self) -> Result<()> {
        check(
            ["private", "public"].contains(&self.visibility.as_str()),
            "Invalid channel visibility",
        )?;
        check(
            self.name
                .as_deref()
                .is_none_or(|v| !v.trim().is_empty() && v.len() <= 100),
            "Use a channel name of 1–100 characters",
        )?;
        // The same limit as the purpose given when the channel is created.
        check(
            self.description.as_deref().is_none_or(|v| v.len() <= 300),
            "Use a description of at most 300 characters",
        )
    }
}
async fn settings(
    State(app): State<Shared>,
    RoutePath((w, chat)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<ChannelSettings>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    app.store.channel_settings(&user, &w, &chat, &input).await?;
    Ok(Json(json!({"ok":true})))
}

pub(crate) mod config;
use super::*;
use axum::{
    Json,
    body::Bytes,
    extract::{DefaultBodyLimit, Path as RoutePath, Query},
    http::HeaderMap,
};
use config::{SYSTEM_GUIDE, SaveAgent};
use serde_json::{Value, json};
pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{id}/requests",
            get(requests),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{id}/requests/{reply}",
            get(request),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles",
            get(list).post(save),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{id}/context-preview",
            get(context_preview),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{id}/knowledge",
            get(knowledge).post(remember),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/prompt",
            post(prompt),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{id}/owner",
            post(transfer),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{id}/avatar",
            post(upload).layer(DefaultBodyLimit::max(2 * 1024 * 1024)),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{id}/avatar/{avatar}",
            get(avatar),
        )
}
#[derive(serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RequestPage {
    before: Option<i64>,
    scope: Option<String>,
    status: Option<String>,
}
async fn requests(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Query(page): Query<RequestPage>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    check(
        page.scope
            .as_deref()
            .is_none_or(|s| ["mine", "visible"].contains(&s)),
        "Invalid request scope",
    )?;
    check(
        page.status
            .as_deref()
            .is_none_or(|s| ["active", "all"].contains(&s)),
        "Invalid request filter",
    )?;
    Ok(Json(
        app.store
            .agent_requests(
                &user,
                &w,
                &id,
                page.scope.as_deref() != Some("visible"),
                page.status.as_deref() != Some("all"),
                page.before,
            )
            .await?,
    ))
}
async fn request(
    State(app): State<Shared>,
    RoutePath((w, id, reply)): RoutePath<(String, String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.agent_request(&user, &w, &id, &reply).await?))
}
async fn list(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let role = workspaces::membership(&app, &user, &w).await?;
    let (models, default_model) = app.store.workspace_models(&user, &w).await?;
    Ok(Json(
        json!({"agents":app.store.agent_profiles(&user,&w).await?,"role":role,"system_prompt":SYSTEM_GUIDE,"providers":[{"id":"openai","name":"OpenAI","configured":hosting::provider_configured(&app, "openai")},{"id":"fireworks-ai","name":"Fireworks","configured":hosting::provider_configured(&app, "fireworks-ai")}],"default_model":default_model.unwrap_or_else(|| app.config.default_model.clone()),"models":models}),
    ))
}
async fn save(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Json(input): Json<SaveAgent>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.save_agent_profile(&user, &w, &input).await?))
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PromptDraft {
    name: String,
    role: String,
    instructions: String,
}
/// "See everything it receives": the exact text an agent with this draft gets,
/// including the locked Cuckoo rules. Nothing is saved.
async fn prompt(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Json(input): Json<PromptDraft>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    workspaces::membership(&app, &user, &w).await?;
    check(
        input.name.len() <= 80
            && input.role.chars().count() <= 80
            && input.instructions.len() <= config::INSTRUCTIONS_LIMIT,
        "Draft is too long",
    )?;
    let settings = config::Settings {
        instructions: input.instructions,
        ..Default::default()
    };
    Ok(Json(
        json!({"prompt":config::instructions(input.name.trim(), &input.role, Some(&settings))}),
    ))
}
async fn transfer(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<crate::identity::postgres::TransferAgent>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store.agent_transfer(&user, &w, &id, &input).await?,
    ))
}
async fn knowledge(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        json!({"notes":app.store.agent_knowledge(&user,&w,&id).await?}),
    ))
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ContextPreview {
    chat: String,
    task: Option<String>,
    claim: Option<String>,
}
async fn context_preview(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Query(input): Query<ContextPreview>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    if let (Some(task), Some(claim)) = (&input.task, &input.claim) {
        return Ok(Json(
            app.store
                .agent_context_detail(&user, &w, &id, &input.chat, task, claim)
                .await?,
        ));
    }
    if input.task.is_some() || input.claim.is_some() {
        return Err(anyhow::anyhow!("Choose an exact attempt").into());
    }
    Ok(Json(
        app.store
            .agent_context_preview(&user, &w, &id, &input.chat)
            .await?,
    ))
}
async fn remember(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<postgres::KnowledgeInput>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    app.store
        .save_agent_knowledge(&user, &w, &id, &input)
        .await?;
    Ok(Json(json!({"ok":true})))
}
async fn upload(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
    bytes: Bytes,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let mime = crate::identity::pictures::picture_mime(&bytes)
        .ok_or_else(|| anyhow::anyhow!("Choose a PNG, JPEG or WebP avatar"))?;
    Ok(Json(
        json!({"avatar":app.store.agent_avatar_save(&user,&w,&id,mime,bytes.to_vec()).await?}),
    ))
}
async fn avatar(
    State(app): State<Shared>,
    RoutePath((w, id, avatar)): RoutePath<(String, String, String)>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let user = identity(&app, &headers).await?;
    let (mime, bytes) = app.store.agent_avatar(&user, &w, &id, &avatar).await?;
    Ok(crate::identity::pictures::picture_response(&mime, bytes)?)
}

pub(crate) mod computer_policy;
mod computer_routes;
pub(crate) mod computer_settings;
pub(crate) mod config;
pub(crate) mod system_prompt;
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
    computer_routes::routes()
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
            "/api/identity/workspaces/{w}/agent-profiles/{id}/pause",
            post(pause),
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
        json!({"agents":app.store.agent_profiles(&user,&w).await?,"role":role,"system_prompt":SYSTEM_GUIDE,"providers":[{"id":"openai","name":"OpenAI","configured":hosting::provider_configured(&app, "openai")},{"id":"fireworks-ai","name":"Fireworks","configured":hosting::provider_configured(&app, "fireworks-ai")}],"default_model":super::models::default_model(&app, default_model),"models":models,"catalog":super::models::offered(&app)?,"web_search":app.store.web_search.is_some(),"computers":app.store.computers.as_ref().map(|c| c.offered()),"whatsapp":super::communications::whatsapp::offered()}),
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
    /// A saved agent: its apps, skills and Library decide the sections.
    #[serde(default)]
    agent: Option<String>,
    /// For a new agent: whether any app is chosen, and its skills.
    #[serde(default)]
    apps: bool,
    #[serde(default)]
    skills: Vec<SkillLine>,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct SkillLine {
    name: String,
    description: String,
}
/// "Everything it receives": the Cuckoo system prompt as sections, marked included
/// or not for this agent, and the exact text of a direct request. Nothing is saved.
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
            && input.instructions.len() <= config::INSTRUCTIONS_LIMIT
            && input.skills.len() <= 20
            && input
                .skills
                .iter()
                .all(|s| s.name.len() <= 64 && s.description.len() <= 1024),
        "Draft is too long",
    )?;
    let mut settings = config::Settings::default();
    let features = match &input.agent {
        Some(id) => {
            app.store.managed_agent(&user, &w, id, false).await?;
            if let Some(saved) = app.store.agent_settings(&w, id).await? {
                settings = saved;
            }
            app.store.prompt_features(&w, id).await?
        }
        None => system_prompt::Features {
            business_actions: input.apps,
            executor: app.store.pooled,
            library: false,
            // A new agent has web search unless its creator switches it off.
            web_search: app.store.web_search.is_some(),
            // Computers are off until the owner turns one on.
            computer: false,
            skills: input
                .skills
                .into_iter()
                .map(|s| (s.name, s.description))
                .collect(),
        },
    };
    settings.instructions = input.instructions;
    let name = input.name.trim();
    let skills: Vec<Value> = features
        .skills
        .iter()
        .map(|(name, description)| json!({"name": name, "description": description}))
        .collect();
    Ok(Json(json!({
        "prompt": system_prompt::prompt(name, &input.role, &settings, &features),
        "version": system_prompt::VERSION,
        "sections": system_prompt::sections(name, &input.role, &features),
        "instructions": config::own(&settings),
        "skills": skills,
    })))
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
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PauseAgent {
    paused: bool,
}
/// Pause or resume one agent: its owner, its managers or an admin (Activity's
/// agent cards use this; the Agents page keeps its own pause switch).
async fn pause(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<PauseAgent>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store
            .agent_pause_set(&user, &w, &id, input.paused)
            .await?,
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

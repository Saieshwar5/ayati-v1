pub(crate) mod config;
use super::*;
use axum::{
    Json,
    body::Bytes,
    extract::{DefaultBodyLimit, Path as RoutePath},
    http::HeaderMap,
};
use config::{SYSTEM_GUIDE, SaveAgent};
use serde_json::{Value, json};
fn pg(app: &App) -> Result<&postgres::PgStore> {
    collaboration::pg(app)
}
pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route(
            "/api/identity/workspaces/{w}/agent-profiles",
            get(list).post(save),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{id}/knowledge",
            get(knowledge).post(remember),
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
async fn list(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let role = workspaces::membership(&app, &user, &w).await?;
    Ok(Json(
        json!({"agents":pg(&app)?.agent_profiles(&user,&w).await?,"role":role,"system_prompt":SYSTEM_GUIDE,"providers":[{"id":"openai","name":"OpenAI","configured":hosting::provider_configured("openai","OPENAI_API_KEY")},{"id":"fireworks-ai","name":"Fireworks","configured":hosting::provider_configured("fireworks-ai","FIREWORKS_API_KEY")}],"default_model":crate::config::Config::from_env()?.model}),
    ))
}
async fn save(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Json(input): Json<SaveAgent>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(pg(&app)?.save_agent_profile(&user, &w, &input).await?))
}
async fn knowledge(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        json!({"notes":pg(&app)?.agent_knowledge(&user,&w,&id).await?}),
    ))
}
async fn remember(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<postgres::KnowledgeInput>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    pg(&app)?
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
    let mime = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        "image/jpeg"
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        "image/webp"
    } else {
        return Err(anyhow::anyhow!("Choose a PNG, JPEG or WebP avatar").into());
    };
    Ok(Json(
        json!({"avatar":pg(&app)?.agent_avatar_save(&user,&w,&id,mime,bytes.to_vec()).await?}),
    ))
}
async fn avatar(
    State(app): State<Shared>,
    RoutePath((w, id, avatar)): RoutePath<(String, String, String)>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let user = identity(&app, &headers).await?;
    let (mime, bytes) = pg(&app)?.agent_avatar(&user, &w, &id, &avatar).await?;
    let mut response = bytes.into_response();
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, mime.parse()?);
    response
        .headers_mut()
        .insert("x-content-type-options", "nosniff".parse()?);
    Ok(response)
}

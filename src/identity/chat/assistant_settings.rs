//! Routes for what a member's assistant knows about them, its regular checks,
//! and drafts it writes for them (schema 129).
use super::*;
use crate::identity::postgres::assistants::settings::{DraftRequest, SaveSettings};

pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route(
            "/api/identity/workspaces/{w}/assistant/settings",
            get(read).post(save),
        )
        .route("/api/identity/workspaces/{w}/assistant/drafts", post(draft))
        .route(
            "/api/identity/workspaces/{w}/assistant/drafts/{reply}",
            get(drafted),
        )
        .route(
            "/api/identity/workspaces/{w}/assistant/drafts/{reply}/used",
            post(used),
        )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Used {
    message: String,
}

async fn read(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.assistant_settings(&user, &w).await?))
}

async fn save(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: axum::http::HeaderMap,
    Json(input): Json<SaveSettings>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    app.store.save_assistant_settings(&user, &w, &input).await?;
    Ok(Json(json!({"ok":true})))
}

/// Ask the assistant for a reply to send in a conversation. Nothing is posted there.
async fn draft(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: axum::http::HeaderMap,
    Json(input): Json<DraftRequest>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let (room, job) = app.store.draft_request(&user, &w, &input).await?;
    app.runtimes.chat_wake.notify_one();
    Ok(Json(json!({"ok":true,"chat":room,"reply":job.reply})))
}

async fn drafted(
    State(app): State<Shared>,
    RoutePath((w, reply)): RoutePath<(String, String)>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.draft(&user, &w, &reply).await?))
}

async fn used(
    State(app): State<Shared>,
    RoutePath((w, reply)): RoutePath<(String, String)>,
    headers: axum::http::HeaderMap,
    Json(input): Json<Used>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    app.store
        .draft_used(&user, &w, &reply, &input.message)
        .await?;
    Ok(Json(json!({"ok":true})))
}

//! Routes for what a member's assistant knows about them, its regular checks,
//! drafts it writes for them (schema 129) and its help setting up an agent
//! (schema 139).
use super::*;
use crate::identity::postgres::assistants::memory::SaveMemory;
use crate::identity::postgres::assistants::settings::{DraftRequest, SaveSettings};
use crate::identity::postgres::assistants::setup::SetupRequest;

pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route(
            "/api/identity/workspaces/{w}/assistant/settings",
            get(read).post(save),
        )
        .route(
            "/api/identity/workspaces/{w}/assistant/memories",
            get(memories).post(remember),
        )
        .route(
            "/api/identity/workspaces/{w}/assistant/memories/{id}/{change}",
            post(change_memory),
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
        .route(
            "/api/identity/workspaces/{w}/assistant/agent-setup",
            get(setup_status).post(setup),
        )
        .route(
            "/api/identity/workspaces/{w}/assistant/agent-setup/{reply}",
            get(setup_drafts),
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

/// The member's own memory list: what their assistant remembers about them.
async fn memories(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.assistant_memories(&user, &w).await?))
}

/// The member adds a line or changes one of theirs; the same id saves once.
async fn remember(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: axum::http::HeaderMap,
    Json(input): Json<SaveMemory>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    app.store.save_assistant_memory(&user, &w, &input).await?;
    Ok(Json(json!({"ok":true})))
}

/// `remove` deletes a line for good; `restore` brings back one the assistant ended.
async fn change_memory(
    State(app): State<Shared>,
    RoutePath((w, id, change)): RoutePath<(String, String, String)>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let restore = match change.as_str() {
        "restore" => true,
        "remove" => false,
        _ => return Err(anyhow::anyhow!("Unknown memory change").into()),
    };
    app.store
        .change_assistant_memory(&user, &w, &id, restore)
        .await?;
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
    if app
        .store
        .draft_used(&user, &w, &reply, &input.message)
        .await?
    {
        app.runtimes.chat_wake.notify_one();
    }
    Ok(Json(json!({"ok":true})))
}

/// Whether the member's assistant can help set up an agent here, and who it is.
async fn setup_status(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.agent_setup_status(&user, &w).await?))
}

/// One turn of the setup chat: the member's assistant is asked in its own room
/// to draft an agent's ground rules and skills. Nothing is saved.
async fn setup(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: axum::http::HeaderMap,
    Json(input): Json<SetupRequest>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let (room, job) = app.store.agent_setup_request(&user, &w, &input).await?;
    app.runtimes.chat_wake.notify_one();
    Ok(Json(json!({"ok":true,"chat":room,"reply":job.reply})))
}

/// The drafts of one turn, once the assistant has answered.
async fn setup_drafts(
    State(app): State<Shared>,
    RoutePath((w, reply)): RoutePath<(String, String)>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.agent_setup(&user, &w, &reply).await?))
}

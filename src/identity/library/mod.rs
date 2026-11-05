pub(in crate::identity) mod context;
mod extract;
mod files;
pub(in crate::identity) mod indexer;
pub(in crate::identity) mod keeping;
pub(crate) mod model;
pub(crate) mod models;
mod office;
pub(crate) mod page_text;
pub(crate) mod passages;
mod storage;
pub(crate) mod tables;
use super::*;
use axum::{
    Json,
    extract::{Path as RoutePath, Query},
};
use model::*;
use serde::Deserialize;
use serde_json::{Value, json};
pub(super) use storage::Storage;

pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route(
            "/api/identity/workspaces/{w}/library",
            get(list)
                .post(save)
                .layer(axum::extract::DefaultBodyLimit::max(300 * 1024)),
        )
        .route(
            "/api/identity/workspaces/{w}/library-folders",
            post(create_folder),
        )
        .route(
            "/api/identity/workspaces/{w}/library/{id}/review",
            get(review).post(review_set),
        )
        .route(
            "/api/identity/workspaces/{w}/library/{id}/keeping",
            get(keeping).post(keeping_set),
        )
        .route("/api/identity/workspaces/{w}/library/{id}/hold", post(hold))
        .route(
            "/api/identity/workspaces/{w}/library/{id}/delete-forever",
            post(delete_forever),
        )
        .route(
            "/api/identity/workspaces/{w}/library-trash/empty",
            post(empty_trash),
        )
        .route(
            "/api/identity/workspaces/{w}/library/{id}/watch",
            get(watch).post(watch_set),
        )
        .route(
            "/api/identity/workspaces/{w}/library/{id}/fields",
            get(fields),
        )
        .route("/api/identity/workspaces/{w}/library-gaps", get(gaps))
        .route(
            "/api/identity/workspaces/{w}/library-answers",
            post(save_answer).layer(axum::extract::DefaultBodyLimit::max(128 * 1024)),
        )
        .route(
            "/api/identity/workspaces/{w}/library-gaps/{id}",
            post(gap_close),
        )
        .route(
            "/api/identity/workspaces/{w}/library-tables",
            post(save_table).layer(axum::extract::DefaultBodyLimit::max(1_200_000)),
        )
        .route(
            "/api/identity/workspaces/{w}/library-always/{agent}",
            get(always).post(always_save),
        )
        .route(
            "/api/identity/workspaces/{w}/library/{id}/suggestions",
            get(suggestions),
        )
        .route(
            "/api/identity/workspaces/{w}/library/{id}/suggestions/{suggestion}",
            post(decide),
        )
        .route("/api/identity/workspaces/{w}/library/{id}", get(detail))
        .route(
            "/api/identity/workspaces/{w}/library/{id}/move",
            post(move_item),
        )
        .route(
            "/api/identity/workspaces/{w}/library/{id}/share",
            post(share),
        )
        .route(
            "/api/identity/workspaces/{w}/library/{id}/action",
            post(action),
        )
        .route(
            "/api/identity/workspaces/{w}/library/{id}/versions",
            get(versions),
        )
        .route(
            "/api/identity/workspaces/{w}/library/{id}/versions/{version}",
            get(version),
        )
        .route("/api/identity/workspaces/{w}/library/{id}/ask", post(ask))
        .route(
            "/api/identity/workspaces/{w}/library/{id}/download",
            get(files::download),
        )
        .route(
            "/api/identity/workspaces/{w}/library-uploads",
            post(files::start),
        )
        .route(
            "/api/identity/workspaces/{w}/library-uploads/{id}",
            get(files::status).post(files::cancel),
        )
        .route(
            "/api/identity/workspaces/{w}/library-uploads/{id}/bytes",
            post(files::upload).layer(axum::extract::DefaultBodyLimit::max(MAX_FILE)),
        )
}
#[derive(Deserialize, Default)]
struct Browse {
    #[serde(default)]
    view: String,
    #[serde(default)]
    q: String,
    #[serde(default)]
    folder: String,
    #[serde(default)]
    offset: i64,
}
async fn list(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: axum::http::HeaderMap,
    Query(input): Query<Browse>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    workspaces::membership(&app, &user, &w).await?;
    let mut result = app
        .store
        .library_list(
            &user,
            &w,
            if input.view.is_empty() {
                "all"
            } else {
                &input.view
            },
            &input.q,
            &input.folder,
            input.offset,
        )
        .await?;
    result["available"] = json!(true);
    result["storage"] = json!(app.library.mode());
    Ok(Json(result))
}
async fn detail(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.library_get(&user, &w, &id).await?))
}
async fn save(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: axum::http::HeaderMap,
    Json(input): Json<SavePage>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.library_save(&user, &w, &input).await?))
}
async fn review(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.library_review(&user, &w, &id).await?))
}
async fn review_set(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: axum::http::HeaderMap,
    Json(input): Json<crate::identity::postgres::Review>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store.library_review_set(&user, &w, &id, &input).await?,
    ))
}
/// An item's legal hold and a folder's keeping rule (LB7).
async fn keeping(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.library_keeping(&user, &w, &id).await?))
}
async fn keeping_set(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: axum::http::HeaderMap,
    Json(input): Json<crate::identity::postgres::Keeping>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store
            .library_keeping_set(&user, &w, &id, &input)
            .await?,
    ))
}
async fn hold(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: axum::http::HeaderMap,
    Json(input): Json<crate::identity::postgres::Hold>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store.library_hold_set(&user, &w, &id, &input).await?,
    ))
}
/// Deletes one trashed item for good, then its stored file (LB7).
async fn delete_forever(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let files = app.store.library_delete_forever(&user, &w, &id).await?;
    keeping::remove(&app, files).await;
    Ok(Json(json!({"deleted":1})))
}
/// Deletes everything the person may in their Trash for good, except held items.
async fn empty_trash(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let (deleted, files) = app.store.library_empty_trash(&user, &w).await?;
    keeping::remove(&app, files).await;
    Ok(Json(json!({"deleted":deleted})))
}
/// Who watches a folder, and setting or stopping it (LB6).
async fn watch(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.library_watch(&user, &w, &id).await?))
}
async fn watch_set(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: axum::http::HeaderMap,
    Json(input): Json<crate::identity::postgres::Watch>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store.library_watch_set(&user, &w, &id, &input).await?,
    ))
}
/// Key fields agents recorded on an item.
async fn fields(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.library_fields(&user, &w, &id).await?))
}
/// Saves an answer from Chat as a Library page (LB5).
async fn save_answer(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: axum::http::HeaderMap,
    Json(input): Json<crate::identity::postgres::SaveAnswer>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store.library_save_answer(&user, &w, &input).await?,
    ))
}
async fn gaps(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.library_gaps(&user, &w).await?))
}
async fn gap_close(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: axum::http::HeaderMap,
    Json(input): Json<crate::identity::postgres::GapAction>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store.library_gap_close(&user, &w, &id, &input).await?,
    ))
}
async fn save_table(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: axum::http::HeaderMap,
    Json(input): Json<crate::identity::postgres::SaveTable>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.library_table_save(&user, &w, &input).await?))
}
async fn always(
    State(app): State<Shared>,
    RoutePath((w, agent)): RoutePath<(String, String)>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.library_always(&user, &w, &agent).await?))
}
async fn always_save(
    State(app): State<Shared>,
    RoutePath((w, agent)): RoutePath<(String, String)>,
    headers: axum::http::HeaderMap,
    Json(input): Json<crate::identity::postgres::AlwaysRead>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store
            .library_always_save(&user, &w, &agent, &input)
            .await?,
    ))
}
async fn suggestions(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.library_suggestions(&user, &w, &id).await?))
}
async fn decide(
    State(app): State<Shared>,
    RoutePath((w, id, suggestion)): RoutePath<(String, String, String)>,
    headers: axum::http::HeaderMap,
    Json(input): Json<crate::identity::postgres::Decision>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store
            .library_decide_suggestion(&user, &w, &id, &suggestion, &input)
            .await?,
    ))
}
async fn create_folder(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: axum::http::HeaderMap,
    Json(input): Json<CreateFolder>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store.library_folder_create(&user, &w, &input).await?,
    ))
}
async fn move_item(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: axum::http::HeaderMap,
    Json(input): Json<MoveItem>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.library_move(&user, &w, &id, &input).await?))
}
async fn share(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: axum::http::HeaderMap,
    Json(input): Json<ShareItem>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.library_share(&user, &w, &id, &input).await?))
}
async fn action(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: axum::http::HeaderMap,
    Json(input): Json<ItemAction>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store.library_action(&user, &w, &id, &input).await?,
    ))
}
async fn versions(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.library_versions(&user, &w, &id).await?))
}
async fn version(
    State(app): State<Shared>,
    RoutePath((w, id, version)): RoutePath<(String, String, i64)>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store.library_version(&user, &w, &id, version).await?,
    ))
}
async fn ask(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: axum::http::HeaderMap,
    Json(input): Json<AskItem>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let result = app.store.library_ask(&user, &w, &id, &input).await?;
    app.runtimes.chat_wake.notify_one();
    Ok(Json(result))
}

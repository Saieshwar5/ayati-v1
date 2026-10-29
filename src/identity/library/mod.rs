pub(in crate::identity) mod context;
mod extract;
mod files;
pub(crate) mod model;
mod storage;
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
        .route("/api/identity/workspaces/{w}/library/{id}", get(detail))
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

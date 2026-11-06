//! Built apps (Apps AP4–AP9, first part): the HTTP boundary. The store authorizes every
//! request; the runnable document is served with a sandboxing content policy, so it
//! never runs with Cuckoo's own origin.
pub(crate) mod files;
pub(crate) mod manifest;
use super::*;
use crate::identity::postgres::{AppFile, AppRelease, DataRequest, NewApp, RoleChange};
use axum::{
    Json,
    extract::{Path as RoutePath, Query},
    http::{HeaderMap, header},
};
use serde::Deserialize;
use serde_json::Value;

pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route("/api/identity/workspaces/{w}/apps/built", post(create))
        .route(
            "/api/identity/workspaces/{w}/apps/built/{id}",
            post(settings),
        )
        .route(
            "/api/identity/workspaces/{w}/apps/built/{id}/files",
            get(list)
                .post(write)
                .layer(axum::extract::DefaultBodyLimit::max(300 * 1024)),
        )
        .route(
            "/api/identity/workspaces/{w}/apps/built/{id}/publish",
            post(publish),
        )
        .route(
            "/api/identity/workspaces/{w}/apps/built/{id}/rollback",
            post(rollback),
        )
        .route(
            "/api/identity/workspaces/{w}/apps/built/{id}/remove",
            post(remove),
        )
        .route("/api/identity/workspaces/{w}/apps/built/{id}/run", get(run))
        .route(
            "/api/identity/workspaces/{w}/apps/built/{id}/data",
            post(data).layer(axum::extract::DefaultBodyLimit::max(64 * 1024)),
        )
        .route(
            "/api/identity/workspaces/{w}/apps/built/{id}/roles",
            get(roles).post(roles_set),
        )
        .route(
            "/api/identity/workspaces/{w}/apps/built/{id}/download",
            get(download),
        )
}

#[derive(Deserialize)]
struct Version {
    #[serde(default)]
    version: i32,
}
#[derive(Deserialize)]
struct Run {
    #[serde(default)]
    preview: bool,
}

async fn create(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Json(input): Json<NewApp>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.built_app_create(&user, &w, &input).await?))
}
async fn settings(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<NewApp>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store.built_app_settings(&user, &w, &id, &input).await?,
    ))
}
async fn list(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    Query(at): Query<Version>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store
            .built_app_files(&user, &w, &id, at.version)
            .await?,
    ))
}
async fn write(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<AppFile>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store.built_app_write(&user, &w, &id, &input).await?,
    ))
}
async fn publish(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<AppRelease>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store.built_app_publish(&user, &w, &id, &input).await?,
    ))
}
async fn rollback(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<AppRelease>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store.built_app_rollback(&user, &w, &id, &input).await?,
    ))
}
async fn remove(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.built_app_remove(&user, &w, &id).await?))
}
/// The app's document, loaded by a sandboxed frame on the Apps page. The response's
/// content policy (set with the other security headers) gives it an opaque origin.
async fn run(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    Query(mode): Query<Run>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let user = identity(&app, &headers).await?;
    let run = app
        .store
        .built_app_run(&user, &w, &id, mode.preview)
        .await?;
    let document = run["document"].as_str().unwrap_or("").to_owned();
    let mut response = Response::new(axum::body::Body::from(document));
    let h = response.headers_mut();
    h.insert(header::CONTENT_TYPE, "text/html; charset=utf-8".parse()?);
    h.insert(header::CACHE_CONTROL, "private, no-store".parse()?);
    Ok(response)
}
async fn download(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    Query(at): Query<Version>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let user = identity(&app, &headers).await?;
    let (name, bytes) = app
        .store
        .built_app_download(&user, &w, &id, at.version)
        .await?;
    let mut response = Response::new(axum::body::Body::from(bytes));
    let h = response.headers_mut();
    h.insert(header::CONTENT_TYPE, "application/zip".parse()?);
    h.insert(
        header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"{name}\"").parse()?,
    );
    h.insert(header::CACHE_CONTROL, "private, no-store".parse()?);
    h.insert("x-content-type-options", "nosniff".parse()?);
    Ok(response)
}

/// One request a running app made through the bridge, relayed by the Apps page.
async fn data(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<DataRequest>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store.built_app_data(&user, &w, &id, &input).await?,
    ))
}
async fn roles(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.built_app_roles(&user, &w, &id).await?))
}
async fn roles_set(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<RoleChange>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store
            .built_app_roles_set(&user, &w, &id, &input)
            .await?,
    ))
}

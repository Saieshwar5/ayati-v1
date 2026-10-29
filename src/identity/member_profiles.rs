//! Routes for workspace profiles: everyone reads, each person edits their own.
use super::*;
use axum::{
    Json,
    body::Bytes,
    extract::{DefaultBodyLimit, Path as RoutePath},
    http::HeaderMap,
};
use pictures::{picture_mime, picture_response};
use postgres::member_profiles::PHOTO_LIMIT;
use serde::Deserialize;
use serde_json::Value;

pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route(
            "/api/identity/workspaces/{w}/my-profile",
            axum::routing::post(save),
        )
        .route(
            "/api/identity/workspaces/{w}/my-profile/photo",
            axum::routing::post(upload).layer(DefaultBodyLimit::max(PHOTO_LIMIT)),
        )
        .route(
            "/api/identity/workspaces/{w}/my-profile/photo/remove",
            axum::routing::post(remove),
        )
        .route(
            "/api/identity/workspaces/{w}/members/{member}/photo/{photo}",
            get(photo),
        )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveProfile {
    display_name: String,
    title: String,
    about: String,
}

async fn save(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Json(input): Json<SaveProfile>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store
            .save_member_profile(&user, &w, &input.display_name, &input.title, &input.about)
            .await?,
    ))
}

async fn upload(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    bytes: Bytes,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let mime =
        picture_mime(&bytes).ok_or_else(|| anyhow::anyhow!("Choose a PNG, JPEG or WebP photo"))?;
    Ok(Json(
        app.store
            .save_member_photo(&user, &w, mime, bytes.to_vec())
            .await?,
    ))
}

async fn remove(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.remove_member_photo(&user, &w).await?))
}

async fn photo(
    State(app): State<Shared>,
    RoutePath((w, member, photo)): RoutePath<(String, String, String)>,
    headers: HeaderMap,
) -> ApiResult<axum::response::Response> {
    let user = identity(&app, &headers).await?;
    let (mime, bytes) = app.store.member_photo(&user, &w, &member, &photo).await?;
    Ok(picture_response(&mime, bytes)?)
}

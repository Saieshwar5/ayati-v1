//! User-chosen labels are independent of authentication and workspace authority.
use super::*;
use axum::{Json, http::HeaderMap};
use serde::Deserialize;

pub(super) fn validate(name: &str) -> Result<&str> {
    let trimmed = name.trim();
    anyhow::ensure!(
        !trimmed.is_empty() && trimmed.chars().count() <= 80 && !name.chars().any(char::is_control),
        "Use a display name between 1 and 80 characters without control characters"
    );
    Ok(trimmed)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Profile {
    display_name: String,
}
pub(super) async fn update(
    State(app): State<Shared>,
    headers: HeaderMap,
    Json(input): Json<Profile>,
) -> ApiResult<Json<User>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store.update_profile(&user, &input.display_name).await?,
    ))
}

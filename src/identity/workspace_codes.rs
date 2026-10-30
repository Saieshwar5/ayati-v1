//! Invite-only early access: platform administrators (from configuration) issue
//! single-use workspace codes; anyone else needs one to create a workspace.
use super::*;
use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use serde::Deserialize;
use serde_json::{Value, json};

/// Administrators are named by verified email in AYATI_PLATFORM_ADMINS, never in the app.
pub(super) fn platform_admin(app: &App, user: &User) -> bool {
    let email = user.email.to_lowercase();
    app.config.platform_admins.contains(&email)
}

async fn admin(app: &App, headers: &HeaderMap) -> ApiResult<User> {
    let user = identity(app, headers).await?;
    if !platform_admin(app, &user) {
        return Err(Error(
            anyhow::anyhow!("Only Cuckoo administrators can manage workspace codes"),
            axum::http::StatusCode::FORBIDDEN,
        ));
    }
    Ok(user)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Check {
    code: String,
}
/// Lets the person name their workspace before the code is used up.
pub(super) async fn check(
    State(app): State<Shared>,
    headers: HeaderMap,
    Json(input): Json<Check>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let expires = app.store.check_workspace_code(&user, &input.code).await?;
    Ok(Json(json!({"expires_at": expires})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Issue {
    note: String,
    days: i64,
}
pub(super) async fn issue(
    State(app): State<Shared>,
    headers: HeaderMap,
    Json(input): Json<Issue>,
) -> ApiResult<Json<Value>> {
    let user = admin(&app, &headers).await?;
    Ok(Json(
        app.store
            .issue_workspace_code(&user, &input.note, input.days)
            .await?,
    ))
}

pub(super) async fn list(State(app): State<Shared>, headers: HeaderMap) -> ApiResult<Json<Value>> {
    admin(&app, &headers).await?;
    Ok(Json(app.store.workspace_codes().await?))
}

pub(super) async fn revoke(
    State(app): State<Shared>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    let user = admin(&app, &headers).await?;
    app.store.revoke_workspace_code(&user, &id).await?;
    Ok(Json(json!({"revoked": true})))
}

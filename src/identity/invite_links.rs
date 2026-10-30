//! Invite links: a public preview, then accept or decline after signing in with
//! the invited email; managers can replace a pending invitation's link.
use super::*;
use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use serde_json::{Value, json};

/// Anyone with the link sees who invited them to what; signed-in viewers also
/// learn whether their email matches.
pub(super) async fn preview(
    State(app): State<Shared>,
    headers: HeaderMap,
    Path(token): Path<String>,
) -> ApiResult<Json<Value>> {
    let viewer = if cookie(&headers, SESSION).is_some() {
        identity(&app, &headers).await.ok()
    } else {
        None
    };
    Ok(Json(
        app.store.invite_preview(&token, viewer.as_ref()).await?,
    ))
}

pub(super) async fn accept(
    State(app): State<Shared>,
    headers: HeaderMap,
    Path(token): Path<String>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        json!({"id": app.store.accept_invite_link(&user, &token).await?}),
    ))
}

pub(super) async fn decline(
    State(app): State<Shared>,
    headers: HeaderMap,
    Path(token): Path<String>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    if let Some(provider) = app.store.decline_invite_link(&user, &token).await? {
        // Best effort: the Cuckoo invitation is already closed either way.
        let _ = app
            .provider
            .request(
                reqwest::Method::POST,
                &format!("/user_management/invitations/{provider}/revoke"),
                Some(json!({})),
            )
            .await;
    }
    Ok(Json(json!({"declined": true})))
}

/// A new link for a pending invitation; the old link stops working.
pub(super) async fn reissue(
    State(app): State<Shared>,
    headers: HeaderMap,
    Path((workspace, invitation)): Path<(String, String)>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let token = app
        .store
        .issue_invite_link(&user, &workspace, &invitation)
        .await?;
    Ok(Json(
        json!({"link": format!("{}/invite/{token}", app.config.origin)}),
    ))
}

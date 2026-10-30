use super::*;
use axum::{
    Json,
    extract::{Path, Query},
    http::HeaderMap,
};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
pub(super) struct LoginQuery {
    invitation_token: Option<String>,
    /// `google` goes straight to Google; otherwise WorkOS AuthKit shows its methods.
    method: Option<String>,
    /// Pre-fills AuthKit's email field (WorkOS `login_hint`).
    email: Option<String>,
    /// Where to land after signing in; only Cuckoo invite links are accepted.
    return_to: Option<String>,
}

/// `/invite/<secret>` only, so sign-in can never redirect to another site.
fn safe_return(path: &str) -> bool {
    path.strip_prefix("/invite/").is_some_and(|token| {
        (20..=100).contains(&token.len())
            && token
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    })
}
pub(super) async fn login(
    State(app): State<Shared>,
    Query(input): Query<LoginQuery>,
) -> ApiResult<Response> {
    if app.simulation.is_some() {
        return Ok(Redirect::to("/index.html?view=simulation").into_response());
    }
    check(
        input
            .invitation_token
            .as_ref()
            .is_none_or(|v| v.len() <= 1024),
        "Invalid invitation",
    )?;
    check(
        input.method.as_deref().is_none_or(|m| m == "google"),
        "Unknown sign-in method",
    )?;
    check(
        input.return_to.as_deref().is_none_or(safe_return),
        "Invalid return page",
    )?;
    let hint = input
        .email
        .as_deref()
        .map(str::trim)
        .filter(|e| !e.is_empty());
    check(
        hint.is_none_or(|e| {
            e.len() <= 254 && e.contains('@') && !e.chars().any(char::is_whitespace)
        }),
        "Enter a valid email",
    )?;
    let token = random_token();
    let state = random_token();
    let verifier = random_token();
    let mut url = reqwest::Url::parse("https://api.workos.com/user_management/authorize")?;
    url.query_pairs_mut().extend_pairs([
        ("client_id", app.provider.client_id.as_str()),
        ("response_type", "code"),
        (
            "provider",
            if input.method.is_some() {
                "GoogleOAuth"
            } else {
                "authkit"
            },
        ),
        (
            "redirect_uri",
            &format!("{}/auth/callback", app.config.origin),
        ),
        ("state", &state),
        ("code_challenge", &digest(&verifier)),
        ("code_challenge_method", "S256"),
    ]);
    if let Some(invitation) = &input.invitation_token {
        url.query_pairs_mut()
            .append_pair("invitation_token", invitation);
    }
    if let Some(email) = hint {
        url.query_pairs_mut().append_pair("login_hint", email);
    }
    login_state::save(
        &app,
        &digest(&token),
        Pending {
            state,
            verifier,
            invitation: input.invitation_token,
            return_to: input.return_to,
            expires: now() + 600,
        },
    )
    .await?;
    let mut response = Redirect::to(url.as_str()).into_response();
    response
        .headers_mut()
        .append(header::SET_COOKIE, set_cookie(&app, FLOW, &token, 600));
    Ok(response)
}
#[derive(Deserialize)]
pub(super) struct Callback {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}
pub(super) async fn callback(
    State(app): State<Shared>,
    headers: HeaderMap,
    Query(input): Query<Callback>,
) -> Response {
    let result: anyhow::Result<(String, Option<String>)> = async {
        let token = cookie(&headers, FLOW).context("Missing sign-in cookie")?;
        let pending = login_state::take(&app, &digest(&token)).await?;
        anyhow::ensure!(
            pending.expires > now() && input.state.as_deref() == Some(&pending.state),
            "Sign-in state mismatch"
        );
        anyhow::ensure!(input.error.is_none(), "Sign-in cancelled");
        let code = input.code.context("Missing authorization code")?;
        anyhow::ensure!(code.len() <= 2048, "Invalid code");
        let data = app
            .provider
            .exchange(&code, &pending.verifier, pending.invitation.as_deref())
            .await?;
        let user = &data["user"];
        anyhow::ensure!(
            user["email_verified"] == true,
            "Verify your email before continuing"
        );
        let provider = user["id"].as_str().context("Missing user")?;
        anyhow::ensure!(
            provider.starts_with("user_")
                && provider
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_'),
            "Invalid provider identity"
        );
        let email = user["email"]
            .as_str()
            .context("Missing email")?
            .to_lowercase();
        let sid = provider::exchanged_session(&data)?;
        let token = random_token();
        // Rotate an existing browser session instead of leaving the old token usable.
        if let Some(old) = cookie(&headers, SESSION) {
            app.store.logout(&digest(&old)).await?;
        }
        app.store
            .login(provider, &email, &digest(&token), &sid)
            .await?;
        Ok((token, pending.return_to))
    }
    .await;
    let mut response = match result {
        Ok((token, return_to)) => {
            let target = return_to.filter(|path| safe_return(path));
            let mut response =
                Redirect::to(target.as_deref().unwrap_or("/?view=account")).into_response();
            response
                .headers_mut()
                .append(header::SET_COOKIE, set_cookie(&app, SESSION, &token, 3600));
            response
        }
        // Deliberately do not expose the provider callback, code, or upstream error body.
        // The log names only our own failure reason (provider errors become one category).
        Err(error) => {
            let message = error.to_string();
            let known = [
                "Missing sign-in cookie",
                "Sign-in expired",
                "Sign-in state mismatch",
                "Sign-in cancelled",
                "Missing authorization code",
                "Verify your email before continuing",
            ];
            let reason = known
                .iter()
                .find(|k| message.starts_with(**k))
                .copied()
                .unwrap_or("Provider exchange or session failed");
            tracing::warn!(reason, "Sign-in callback refused");
            Redirect::to("/index.html?view=account&login_error=1").into_response()
        }
    };
    response
        .headers_mut()
        .append(header::SET_COOKIE, set_cookie(&app, FLOW, "", 0));
    response
}
pub(super) async fn session(State(app): State<Shared>, headers: HeaderMap) -> Response {
    if cookie(&headers, SESSION).is_none() {
        return Json(json!({"user":null})).into_response();
    }
    match identity(&app, &headers).await {
        Ok(user) => match workspaces::session_snapshot(&app, &user).await {
            Ok(value) => Json(value).into_response(),
            Err(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error":"Workspace data unavailable"})),
            )
                .into_response(),
        },
        Err(error) => error.into_response(),
    }
}
pub(super) async fn logout(State(app): State<Shared>, headers: HeaderMap) -> ApiResult<Response> {
    let mut revoked = true;
    if let Some(token) = cookie(&headers, SESSION) {
        let hash = digest(&token);
        let session = app.store.session(&hash).await?;
        app.store.logout(&hash).await?;
        if let Some((_, _, sid)) = session {
            revoked = app
                .provider
                .request(
                    reqwest::Method::POST,
                    "/user_management/sessions/revoke",
                    Some(json!({"session_id":sid})),
                )
                .await
                .is_ok();
        }
    }
    let mut response = Json(json!({"ok":true,"provider_session_revoked":revoked})).into_response();
    response
        .headers_mut()
        .append(header::SET_COOKIE, set_cookie(&app, SESSION, "", 0));
    Ok(response)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Create {
    name: String,
    request_id: String,
    /// Required unless the person is a platform administrator (invite-only early access).
    #[serde(default)]
    code: Option<String>,
}
pub(super) async fn create(
    State(app): State<Shared>,
    headers: HeaderMap,
    Json(input): Json<Create>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let id = if super::workspace_codes::platform_admin(&app, &user) {
        app.store
            .create(&user, &input.name, &input.request_id)
            .await?
    } else {
        let code = input.code.as_deref().filter(|code| !code.trim().is_empty());
        let code = code
            .ok_or_else(|| anyhow::anyhow!("An invite code is required to create a workspace"))?;
        app.store
            .create_with_code(&user, &input.name, &input.request_id, code)
            .await?
    };
    Ok(Json(json!({"id": id})))
}
pub(super) async fn people(
    State(app): State<Shared>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.people(&user, &id).await?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Invite {
    email: String,
    role: String,
}
pub(super) async fn invite(
    State(app): State<Shared>,
    headers: HeaderMap,
    Path(workspace): Path<String>,
    Json(input): Json<Invite>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let email = input.email.trim().to_lowercase();
    let id = app
        .store
        .reserve_invite(&user, &workspace, &email, &input.role)
        .await?;
    // WorkOS sends an application invitation. Ayati owns workspace membership;
    // signing in alone never auto-accepts any of the user's workspace invitations.
    let result = app
        .provider
        .request(
            reqwest::Method::POST,
            "/user_management/invitations",
            Some(json!({"email":email,"expires_in_days":7})),
        )
        .await;
    let provider_id = result.as_ref().ok().and_then(|v| v["id"].as_str());
    app.store.delivered(&id, provider_id).await?;
    check(
        provider_id.is_some(),
        "Email delivery could not be confirmed. No access was granted. Cancel this invitation before trying again.",
    )?;
    // The owner can also send the Cuckoo link themselves (shown once).
    let token = app.store.issue_invite_link(&user, &workspace, &id).await?;
    Ok(Json(
        json!({"id": id, "link": format!("{}/invite/{token}", app.config.origin)}),
    ))
}
pub(super) async fn accept(
    State(app): State<Shared>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(json!({"id":app.store.accept(&user,&id).await?})))
}
pub(super) async fn decline(
    State(app): State<Shared>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    if let Some(provider) = app.store.decline(&user, &id).await? {
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
pub(super) async fn cancel(
    State(app): State<Shared>,
    headers: HeaderMap,
    Path((workspace, id)): Path<(String, String)>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let provider = app.store.cancel(&user, &workspace, &id).await?;
    let mut revoked = true;
    if let Some(provider) = provider {
        revoked = app
            .provider
            .request(
                reqwest::Method::POST,
                &format!("/user_management/invitations/{provider}/revoke"),
                Some(json!({})),
            )
            .await
            .is_ok();
    }
    Ok(Json(
        json!({"ok":true,"provider_invitation_revoked":revoked}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Change {
    role: Option<String>,
}
pub(super) async fn change(
    State(app): State<Shared>,
    headers: HeaderMap,
    Path((workspace, target)): Path<(String, String)>,
    Json(input): Json<Change>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    app.store
        .change(&user, &workspace, &target, input.role.as_deref())
        .await?;
    Ok(Json(json!({"ok":true})))
}

//! Phones signed in with device sessions: the bearer token they send, the
//! list a person sees (Profile › Devices) and signing one out.
use super::*;
use axum::{Json, extract::Path, http::HeaderMap};
use serde_json::{Value, json};

/// The token from `Authorization: Bearer <token>`. Tokens are Cuckoo's own
/// random values, so anything longer or outside base64url is refused.
pub(super) fn bearer(headers: &HeaderMap) -> Option<String> {
    let mut values = headers.get_all(header::AUTHORIZATION).iter();
    let value = values.next()?.to_str().ok()?;
    if values.next().is_some() {
        return None;
    }
    let token = value.strip_prefix("Bearer ")?;
    ((20..=256).contains(&token.len())
        && token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'))
    .then(|| token.to_owned())
}

/// A phone's request: a bearer token and no cookie. Only these skip the
/// browser Origin check, since no cookie means no ambient credential.
pub(super) fn phone_request(headers: &HeaderMap) -> bool {
    headers.get(header::COOKIE).is_none() && bearer(headers).is_some()
}

pub(super) async fn list(State(app): State<Shared>, headers: HeaderMap) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let current = phone_request(&headers)
        .then(|| bearer(&headers).map(|token| digest(&token)))
        .flatten();
    Ok(Json(
        json!({"devices": app.store.devices(&user, current.as_deref()).await?}),
    ))
}

pub(super) async fn revoke(
    State(app): State<Shared>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    check(id.len() <= 64, "Device not found")?;
    let session = app
        .store
        .revoke_device(&user, &id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Device not found"))?;
    Ok(Json(
        json!({"ok": true, "provider_session_revoked": end_provider_session(&app, &session).await}),
    ))
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PushInput {
    expo_token: String,
    #[serde(default)]
    native_token: Option<String>,
}

/// The phone turns notifications on for itself (mobile plan M4).
pub(super) async fn register_push(
    State(app): State<Shared>,
    headers: HeaderMap,
    Json(input): Json<PushInput>,
) -> ApiResult<Json<Value>> {
    let token = phone_token(&headers)?;
    identity(&app, &headers).await?;
    app.store
        .register_push(
            &digest(&token),
            &input.expo_token,
            input.native_token.as_deref(),
        )
        .await?;
    Ok(Json(json!({"ok": true})))
}

pub(super) async fn remove_push(
    State(app): State<Shared>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let token = phone_token(&headers)?;
    identity(&app, &headers).await?;
    app.store.remove_push(&digest(&token)).await?;
    Ok(Json(json!({"ok": true})))
}

/// Only a phone's own device session turns its notifications on or off.
fn phone_token(headers: &HeaderMap) -> ApiResult<String> {
    if !phone_request(headers) {
        return Err(anyhow::anyhow!("Only the phone app changes its notifications").into());
    }
    bearer(headers).ok_or_else(Error::unauthorized)
}

/// Sign out the phone making this request. `None` when it is not a phone.
pub(super) async fn sign_out(app: &App, headers: &HeaderMap) -> ApiResult<Option<bool>> {
    if !phone_request(headers) {
        return Ok(None);
    }
    let Some(token) = bearer(headers) else {
        return Ok(None);
    };
    let revoked = match app.store.logout_device(&digest(&token)).await? {
        Some(session) => end_provider_session(app, &session).await,
        None => true,
    };
    Ok(Some(revoked))
}

/// Best effort: the device session is already gone either way.
async fn end_provider_session(app: &App, session: &str) -> bool {
    app.provider
        .request(
            reqwest::Method::POST,
            "/user_management/sessions/revoke",
            Some(json!({"session_id": session})),
        )
        .await
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for (name, value) in pairs {
            headers.append(
                axum::http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                value.parse().unwrap(),
            );
        }
        headers
    }

    #[test]
    fn only_a_well_formed_bearer_without_a_cookie_is_a_phone() {
        let token = "abcdefghijklmnopqrstuvwxyz0123456789-_AB";
        let bearer_only = headers(&[("authorization", &format!("Bearer {token}"))]);
        assert_eq!(bearer(&bearer_only).as_deref(), Some(token));
        assert!(phone_request(&bearer_only));
        let with_cookie = headers(&[
            ("authorization", &format!("Bearer {token}")),
            ("cookie", "ayati_identity=x"),
        ]);
        assert!(!phone_request(&with_cookie));
        for value in [
            format!("bearer {token}"),
            "Bearer short".into(),
            format!("Bearer {token}!"),
            format!("Basic {token}"),
        ] {
            assert!(
                bearer(&headers(&[("authorization", &value)])).is_none(),
                "{value}"
            );
        }
        let twice = headers(&[
            ("authorization", &format!("Bearer {token}")),
            ("authorization", &format!("Bearer {token}")),
        ]);
        assert!(bearer(&twice).is_none());
    }
}

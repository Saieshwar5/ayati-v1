//! Device sessions: a phone signs in by scanning a one-time code shown on the
//! web, then sends `Authorization: Bearer <token>`. A bearer token goes through
//! [`identity`] like a cookie, so every membership check applies unchanged.
//! See `runtime/device-sessions.md`.
use super::*;
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, Method},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Deserialize;
use serde_json::{Value, json};

const REDEEM: &str = "/api/identity/devices/redeem";

pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route("/api/identity/devices", get(list))
        .route("/api/identity/devices/link-code", post(create_code))
        .route(REDEEM, post(redeem))
        .route("/api/identity/devices/{id}/revoke", post(revoke))
}

/// The device token from a single `Authorization: Bearer` header, when it has
/// the shape [`random_token`] makes.
fn bearer(headers: &HeaderMap) -> Option<&str> {
    let mut values = headers.get_all(header::AUTHORIZATION).iter();
    let value = values.next()?;
    if values.next().is_some() {
        return None;
    }
    let token = value.to_str().ok()?.strip_prefix("Bearer ")?;
    (token.len() == 43
        && token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'))
    .then_some(token)
}

/// A request the phone app makes: it carries no session cookie, so nothing
/// ambient authenticates it and the browser Origin check (a CSRF defence) does
/// not apply. That is a bearer request, or redeeming a code. A browser cannot
/// send an Authorization header to another site without a CORS preflight,
/// which Cuckoo never grants.
pub(super) fn app_request(method: &Method, path: &str, headers: &HeaderMap) -> bool {
    cookie(headers, SESSION).is_none()
        && (bearer(headers).is_some() || (method == Method::POST && path == REDEEM))
}

/// [`identity`] for requests without a session cookie.
pub(super) async fn bearer_identity(app: &App, headers: &HeaderMap) -> ApiResult<User> {
    let token = bearer(headers).ok_or_else(Error::unauthorized)?;
    let session = app
        .store
        .device_session(&digest(token))
        .await?
        .ok_or_else(Error::unauthorized)?;
    if app.simulation.is_some() {
        check(
            session.provider.starts_with("simulation_employee_"),
            "Invalid simulation session",
        )?;
    } else if !app
        .provider
        .account_valid_checked(
            &session.provider,
            &session.user.email,
            READING.try_with(|reading| *reading).unwrap_or(false),
        )
        .await
        .map_err(Error::unavailable)?
    {
        return Err(Error::unauthorized());
    }
    if session.last_used < now() - 60 {
        app.store.touch_device(&session.id).await?;
    }
    Ok(session.user)
}

/// Only a web (cookie) session makes codes: a signed-in phone must not sign
/// in more phones.
async fn create_code(State(app): State<Shared>, headers: HeaderMap) -> ApiResult<Json<Value>> {
    if cookie(&headers, SESSION).is_none() {
        return Err(Error(
            anyhow::anyhow!("Make a phone sign-in code from Cuckoo on the web"),
            StatusCode::FORBIDDEN,
        ));
    }
    let user = identity(&app, &headers).await?;
    let Some((code, expires)) = app.store.create_link_code(&user).await? else {
        return Err(Error(
            anyhow::anyhow!("You made many sign-in codes this hour. Try again later."),
            StatusCode::TOO_MANY_REQUESTS,
        ));
    };
    let link = sign_in_link(&app.config.origin, &code);
    Ok(Json(json!({
        "code": code,
        "expires": expires,
        "expires_in": postgres::device_sessions::CODE_SECONDS,
        "link": link,
        "qr": qr_image(&link)?,
    })))
}

/// `cuckoo://sign-in?server=<origin>&code=<code>`, the QR code's text.
fn sign_in_link(origin: &str, code: &str) -> String {
    let mut url = reqwest::Url::parse("cuckoo://sign-in").expect("static URL");
    url.query_pairs_mut()
        .append_pair("server", origin)
        .append_pair("code", code);
    url.into()
}

/// The QR code as an SVG data URL, dark on white in both themes so phone
/// cameras read it. An image cannot run script, unlike inline SVG markup.
fn qr_image(text: &str) -> anyhow::Result<String> {
    use qrcode::{EcLevel, QrCode, render::svg};
    let code = QrCode::with_error_correction_level(text.as_bytes(), EcLevel::M)?;
    let image = code
        .render::<svg::Color>()
        .min_dimensions(240, 240)
        .dark_color(svg::Color("#000000"))
        .light_color(svg::Color("#ffffff"))
        .build();
    Ok(format!(
        "data:image/svg+xml;base64,{}",
        STANDARD.encode(image)
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Redeem {
    code: String,
    device_name: String,
    platform: String,
}
/// No session: the code is the credential. The token is returned once.
async fn redeem(State(app): State<Shared>, Json(input): Json<Redeem>) -> ApiResult<Json<Value>> {
    let (token, device) = app
        .store
        .redeem_link_code(&input.code, &input.device_name, &input.platform)
        .await?;
    Ok(Json(json!({"token": token, "device": device})))
}

async fn list(State(app): State<Shared>, headers: HeaderMap) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    // The phone asking sees itself marked; a browser is never a listed device.
    let current = cookie(&headers, SESSION)
        .is_none()
        .then(|| bearer(&headers).map(digest))
        .flatten();
    Ok(Json(app.store.devices(&user, current.as_deref()).await?))
}

/// Your own devices only; a phone may sign itself out.
async fn revoke(
    State(app): State<Shared>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    if !app.store.revoke_device(&user, &id).await? {
        return Err(Error(
            anyhow::anyhow!("That device is not signed in"),
            StatusCode::NOT_FOUND,
        ));
    }
    Ok(Json(json!({"revoked": true})))
}

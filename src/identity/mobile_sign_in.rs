//! Sign-in for the phone app (mobile plan M1).
//!
//! 1. The app makes a PKCE verifier and opens `/auth/mobile/start?challenge=…`
//!    in the system browser. Cuckoo runs its normal WorkOS sign-in.
//! 2. The callback hands the browser back to the app with a one-time code that
//!    lasts 60 seconds: through the universal link `/auth/mobile/return`, or a
//!    development build's own scheme (`AYATI_MOBILE_APP_SCHEME`).
//! 3. The app posts the code and verifier to `/api/identity/mobile-sessions` and
//!    gets a bearer token for `Authorization: Bearer`. Without the verifier,
//!    which never leaves the app, an intercepted code is useless.
use super::*;
use crate::identity::postgres::device_sessions::Device;
use axum::{Json, extract::Query, http::HeaderValue};
use serde::Deserialize;
use serde_json::{Value, json};

/// The phone app's routes: sign-in, devices, notifications and the universal-link files.
pub(super) fn routes() -> axum::Router<Shared> {
    axum::Router::new()
        .route("/auth/mobile/start", get(start))
        .route("/auth/mobile/return", get(return_page))
        .route("/api/identity/mobile-sessions", post(exchange))
        .route("/api/identity/devices", get(super::devices::list))
        .route(
            "/api/identity/devices/{id}/revoke",
            post(super::devices::revoke),
        )
        .route(
            "/api/identity/devices/current/push",
            post(super::devices::register_push),
        )
        .route(
            "/api/identity/devices/current/push/remove",
            post(super::devices::remove_push),
        )
        .route(
            "/.well-known/apple-app-site-association",
            get(apple_site_association),
        )
        .route("/.well-known/assetlinks.json", get(asset_links))
}

#[derive(Deserialize)]
pub(super) struct Start {
    /// `BASE64URL(SHA-256(verifier))`, as in RFC 7636's S256 method.
    challenge: String,
    /// `google` goes straight to Google; otherwise AuthKit shows its methods.
    method: Option<String>,
    email: Option<String>,
}

pub(super) async fn start(
    State(app): State<Shared>,
    Query(input): Query<Start>,
) -> ApiResult<Response> {
    check(
        app.simulation.is_none(),
        "The phone app needs a real sign-in",
    )?;
    check(
        input.challenge.len() == 43 && url_safe(&input.challenge),
        "Invalid sign-in challenge",
    )?;
    check(
        input.method.as_deref().is_none_or(|m| m == "google"),
        "Unknown sign-in method",
    )?;
    let hint = routes::login_hint(input.email.as_deref())?;
    // Invitations are accepted on the web or by email; the app only signs in.
    routes::authorize(
        &app,
        input.method.is_some(),
        hint,
        None,
        None,
        Some(input.challenge),
    )
    .await
}

/// After WorkOS signed the person in: a one-time code for the app, and the
/// address that hands it over.
pub(super) async fn finish(
    app: &App,
    provider: &str,
    email: &str,
    provider_session: &str,
    challenge: &str,
) -> Result<String> {
    let code = random_token();
    app.store
        .issue_mobile_code(provider, email, &digest(&code), provider_session, challenge)
        .await?;
    Ok(return_url(app, "code", &code))
}

/// Where the browser goes back to the app, with one query parameter.
pub(super) fn return_url(app: &App, key: &str, value: &str) -> String {
    let base = match &app.config.mobile.scheme {
        Some(scheme) => format!("{scheme}://auth/return"),
        None => format!("{}/auth/mobile/return", app.config.origin),
    };
    let mut url = reqwest::Url::parse(&base).expect("validated mobile return address");
    url.query_pairs_mut().append_pair(key, value);
    url.into()
}

/// The universal link's page, shown only when the app did not open it.
pub(super) async fn return_page() -> Response {
    (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <title>Return to Cuckoo</title></head><body>\
         <h1>Return to the Cuckoo app</h1>\
         <p>You are signed in. Go back to the Cuckoo app to continue. \
         If it did not open, close this page and sign in from the app again.</p>\
         </body></html>",
    )
        .into_response()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Exchange {
    code: String,
    verifier: String,
    device_name: String,
    platform: String,
}

pub(super) async fn exchange(
    State(app): State<Shared>,
    Json(input): Json<Exchange>,
) -> ApiResult<Json<Value>> {
    check(
        (20..=100).contains(&input.code.len()) && url_safe(&input.code),
        crate::identity::postgres::device_sessions::INVALID_CODE,
    )?;
    // RFC 7636: 43 to 128 unreserved characters.
    check(
        (43..=128).contains(&input.verifier.len())
            && input
                .verifier
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~')),
        "Invalid sign-in verifier",
    )?;
    let device = device(&input.device_name, &input.platform)?;
    let token = random_token();
    let issued = app
        .store
        .redeem_mobile_code(
            &digest(&input.code),
            &input.verifier,
            &digest(&token),
            &device,
        )
        .await?;
    Ok(Json(json!({
        "token": token,
        "id": issued.id,
        "expires": issued.expires,
    })))
}

fn device(name: &str, platform: &str) -> Result<Device> {
    let name = name.trim();
    check(
        (1..=80).contains(&name.chars().count()) && !name.chars().any(char::is_control),
        "Use a device name between 1 and 80 characters",
    )?;
    check(
        matches!(platform, "ios" | "android"),
        "Platform must be ios or android",
    )?;
    Ok(Device {
        name: name.into(),
        platform: platform.into(),
    })
}

fn url_safe(value: &str) -> bool {
    value
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// iOS universal links and the app's shared web credentials for this domain.
pub(super) async fn apple_site_association(State(app): State<Shared>) -> Response {
    let ids = &app.config.mobile.ios_app_ids;
    if ids.is_empty() {
        return StatusCode::NOT_FOUND.into_response();
    }
    json_file(json!({
        "applinks": {"details": [{
            "appIDs": ids,
            "components": [{"/": "/auth/mobile/return", "comment": "Sign-in returns to the app"}],
        }]},
        "webcredentials": {"apps": ids},
    }))
}

/// Android App Links for this domain.
pub(super) async fn asset_links(State(app): State<Shared>) -> Response {
    let Some((package, certificates)) = &app.config.mobile.android else {
        return StatusCode::NOT_FOUND.into_response();
    };
    json_file(json!([{
        "relation": ["delegate_permission/common.handle_all_urls"],
        "target": {
            "namespace": "android_app",
            "package_name": package,
            "sha256_cert_fingerprints": certificates,
        },
    }]))
}

fn json_file(value: Value) -> Response {
    (
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        )],
        value.to_string(),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_names_and_platforms_are_checked() {
        assert_eq!(
            device("  Asha's iPhone ", "ios").unwrap().name,
            "Asha's iPhone"
        );
        assert!(device("Pixel", "android").is_ok());
        for (name, platform) in [("", "ios"), ("x\ny", "ios"), ("Phone", "web")] {
            assert!(device(name, platform).is_err());
        }
        assert!(device(&"a".repeat(81), "ios").is_err());
    }
}

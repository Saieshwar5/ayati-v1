//! Isolated local identity server. It never mounts the local-owner agent API.
mod provider;
mod routes;
pub mod store;

use anyhow::{Context, Result};
use axum::{
    Router,
    body::Body,
    extract::State,
    http::{Request, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use provider::{Provider, digest, random_token};
use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex},
};
use store::{Store, User, now};
use tower_http::services::ServeDir;

struct Pending {
    state: String,
    verifier: String,
    invitation: Option<String>,
    expires: i64,
}
struct App {
    provider: Provider,
    store: Store,
    origin: String,
    pending: Mutex<HashMap<String, Pending>>,
}
type Shared = Arc<App>;
type ApiResult<T> = Result<T, Error>;
struct Error(anyhow::Error, StatusCode);
impl Error {
    fn unauthorized() -> Self {
        Self(
            anyhow::anyhow!("Session ended or identity changed. Sign in again."),
            StatusCode::UNAUTHORIZED,
        )
    }
    fn unavailable(_: anyhow::Error) -> Self {
        Self(
            anyhow::anyhow!("Authentication service unavailable. Try again shortly."),
            StatusCode::SERVICE_UNAVAILABLE,
        )
    }
}
impl<E: Into<anyhow::Error>> From<E> for Error {
    fn from(value: E) -> Self {
        Self(value.into(), StatusCode::BAD_REQUEST)
    }
}
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        (
            self.1,
            axum::Json(serde_json::json!({"error":self.0.to_string()})),
        )
            .into_response()
    }
}
const SESSION: &str = "ayati_identity";
const FLOW: &str = "ayati_login";
fn cookie(headers: &axum::http::HeaderMap, name: &str) -> Option<String> {
    let mut matches = headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|v| v.trim().split_once('='))
        .filter(|(key, _)| *key == name);
    let value = matches.next()?.1.to_owned();
    if matches.next().is_some() || value.len() > 256 {
        return None;
    }
    Some(value)
}
fn set_cookie(name: &str, value: &str, age: u32) -> axum::http::HeaderValue {
    // This server binds only to loopback HTTP. Hosted deployment requires Secure cookies + HTTPS.
    format!("{name}={value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={age}")
        .parse()
        .unwrap()
}
async fn identity(app: &App, headers: &axum::http::HeaderMap) -> ApiResult<User> {
    let token = cookie(headers, SESSION).ok_or_else(Error::unauthorized)?;
    let (user, provider, sid) = app
        .store
        .session(&digest(&token))?
        .ok_or_else(Error::unauthorized)?;
    if !app
        .provider
        .session_active(&provider, &sid)
        .await
        .map_err(Error::unavailable)?
    {
        return Err(Error::unauthorized());
    }
    let current = app
        .provider
        .request(
            reqwest::Method::GET,
            &format!("/user_management/users/{provider}"),
            None,
        )
        .await
        .map_err(Error::unavailable)?;
    if !(current["email_verified"] == true
        && current["email"]
            .as_str()
            .is_some_and(|email| email.to_lowercase() == user.email))
    {
        return Err(Error::unauthorized());
    }
    Ok(user)
}
pub async fn serve(data: &Path, port: u16) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let provider = Provider::from_env()?;
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port))
        .await
        .context("Identity port is in use. Stop the other server or choose AYATI_PORT")?;
    let data = data.join("identity");
    std::fs::create_dir_all(&data)?;
    std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o700))?;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(data.join("server.lock"))?;
    lock.try_lock()
        .context("Another identity server uses this data directory")?;
    let app = Arc::new(App {
        provider,
        store: Store::open(&data.join("identity.sqlite"))?,
        origin: format!("http://127.0.0.1:{port}"),
        pending: Mutex::new(HashMap::new()),
    });
    tracing::info!("Identity workspace: {}/?view=account", app.origin);
    axum::serve(listener, router(app))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
fn router(app: Shared) -> Router {
    Router::new()
        .route(
            "/",
            get(|| async { Redirect::to("/index.html?view=account") }),
        )
        .route("/auth/login", get(routes::login))
        .route("/auth/callback", get(routes::callback))
        .route("/api/identity/session", get(routes::session))
        .route("/api/identity/logout", post(routes::logout))
        .route("/api/identity/workspaces", post(routes::create))
        .route("/api/identity/workspaces/{id}/people", get(routes::people))
        .route(
            "/api/identity/workspaces/{id}/invitations",
            post(routes::invite),
        )
        .route(
            "/api/identity/workspaces/{id}/invitations/{invitation}/cancel",
            post(routes::cancel),
        )
        .route(
            "/api/identity/invitations/{id}/accept",
            post(routes::accept),
        )
        .route(
            "/api/identity/workspaces/{id}/members/{user}",
            post(routes::change),
        )
        .fallback_service(ServeDir::new("web/dist"))
        .layer(axum::extract::DefaultBodyLimit::max(16 * 1024))
        .layer(middleware::from_fn_with_state(app.clone(), guard))
        .with_state(app)
}
async fn guard(State(app): State<Shared>, request: Request<Body>, next: Next) -> Response {
    let headers = request.headers();
    let host = app.origin.trim_start_matches("http://");
    if headers.get(header::HOST).and_then(|v| v.to_str().ok()) != Some(host) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if request.method() != axum::http::Method::GET
        && (headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) != Some(app.origin.as_str())
            || headers.get("x-ayati-request").and_then(|v| v.to_str().ok()) != Some("identity"))
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    let path = request.uri().path();
    // No fallback into old local-owner APIs, even for authenticated users.
    if path.starts_with("/api/") && !path.starts_with("/api/identity/") {
        return StatusCode::NOT_FOUND.into_response();
    }
    let mut response = next.run(request).await;
    let h = response.headers_mut();
    h.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    h.insert("referrer-policy", "no-referrer".parse().unwrap());
    h.insert("x-content-type-options", "nosniff".parse().unwrap());
    h.insert("content-security-policy","default-src 'self'; connect-src 'self'; img-src 'self' data:; style-src 'self'; script-src 'self'; object-src 'none'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'".parse().unwrap());
    response
}

#[cfg(test)]
mod tests;

fn check(condition: bool, message: &str) -> anyhow::Result<()> {
    anyhow::ensure!(condition, "{message}");
    Ok(())
}

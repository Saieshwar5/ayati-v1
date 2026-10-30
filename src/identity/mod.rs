//! Authenticated workspace gateway. Legacy APIs remain inaccessible without workspace scope.
mod agents;
mod attention;
mod automations;
mod chat;
mod collaboration;
mod communications;
pub(crate) mod control;
mod events;
pub mod executor;
mod handoffs;
mod home_panels;
mod identifier;
pub(crate) use identifier::identifier;
pub(crate) mod archive;
pub(crate) mod data_cleanup;
mod hosting;
mod interactions;
mod library;
mod login_state;
mod member_profiles;
mod metrics;
mod pictures;
pub mod postgres;
mod private_config;
mod process_lock;
mod profile;
mod provider;
mod rehearsals;
mod routes;
mod settings;
mod shutdown;
pub mod simulation;
pub(crate) mod start_trace;
pub mod user;
mod worker_control;
mod workspace_codes;
mod workspaces;

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
use std::{collections::HashMap, path::Path, sync::Arc};
use tower_http::services::ServeDir;
use user::{User, now};

struct Pending {
    state: String,
    verifier: String,
    invitation: Option<String>,
    expires: i64,
}
struct App {
    config: Arc<crate::config::Controller>,
    library: library::Storage,
    provider: Provider,
    store: postgres::PgStore,
    runtimes: workspaces::Runtimes,
    simulation: Option<simulation::Simulation>,
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
        let error = value.into();
        let status = if error.is::<library::model::Denied>() {
            StatusCode::NOT_FOUND
        } else if error.is::<library::model::Conflict>() {
            StatusCode::CONFLICT
        } else if error.is::<chat::model::QueueFull>()
            || error.is::<chat::model::BackgroundQueueFull>()
        {
            StatusCode::TOO_MANY_REQUESTS
        } else {
            StatusCode::BAD_REQUEST
        };
        Self(error, status)
    }
}
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        if self.0.downcast_ref::<sea_orm::DbErr>().is_some() {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                axum::Json(
                    serde_json::json!({"error":"Workspace database unavailable. Retry shortly."}),
                ),
            )
                .into_response();
        }
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
fn set_cookie(app: &App, name: &str, value: &str, age: u32) -> axum::http::HeaderValue {
    let secure = if hosting::hosted(app) { "; Secure" } else { "" };
    format!("{name}={value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={age}{secure}")
        .parse()
        .unwrap()
}
async fn identity(app: &App, headers: &axum::http::HeaderMap) -> ApiResult<User> {
    let token = cookie(headers, SESSION).ok_or_else(Error::unauthorized)?;
    let (user, provider, sid) = app
        .store
        .session(&digest(&token))
        .await?
        .ok_or_else(Error::unauthorized)?;
    if app.simulation.is_some() {
        check(
            provider.starts_with("simulation_employee_") && sid == "local-simulation",
            "Invalid simulation session",
        )?;
        return Ok(user);
    }
    if !app
        .provider
        .identity_valid(&provider, &sid, &user.email)
        .await
        .map_err(Error::unavailable)?
    {
        return Err(Error::unauthorized());
    }
    Ok(user)
}
/// Process-wide settings read by code that has no `App`: start tracing and the
/// operator-owned connector and event-source catalogs.
fn configure_process(config: &crate::config::Controller) {
    start_trace::enable(config.trace_start);
    control::catalog::configure(config.mcp_servers_file.clone());
    events::catalog::configure(config.event_sources_file.clone());
}

pub async fn serve(config: crate::config::Controller) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let (client_id, key) = config.workos.clone().context("WorkOS is not configured")?;
    let provider = Provider::new(client_id, key)?;
    let listener = tokio::net::TcpListener::bind((config.bind, config.common.port))
        .await
        .context("Identity port is in use. Stop the other server or choose AYATI_PORT")?;
    std::fs::create_dir_all(&config.common.data_dir)?;
    let installation = config.common.data_dir.canonicalize()?;
    let data = installation.join("identity");
    std::fs::create_dir_all(&data)?;
    std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o700))?;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(data.join("server.lock"))?;
    lock.try_lock()
        .context("Another identity server uses this data directory")?;
    let store = postgres::PgStore::open(&config).await?;
    let runtimes = workspaces::Runtimes::new(data.join("workspaces"));
    configure_process(&config);
    let app = Arc::new(App {
        library: library::Storage::from_settings(&config.library)?,
        provider,
        store,
        config: Arc::new(config),
        runtimes,
        simulation: None,
    });
    tracing::info!("Identity workspace: {}/?view=account", app.config.origin);
    let control = worker_control::start(app.clone()).await?;
    chat::start_dispatcher(app.clone());
    automations::start(app.clone()).await;
    metrics::start(app.clone());
    data_cleanup::start(app.clone());
    shutdown::serve(app, listener, control).await
}
fn router(app: Shared) -> Router {
    Router::new()
        .merge(agents::routes())
        .merge(member_profiles::routes())
        .merge(control::routes())
        .merge(attention::routes())
        .merge(home_panels::routes())
        .merge(automations::routes())
        .merge(events::routes())
        .merge(rehearsals::routes())
        .merge(chat::routes())
        .merge(interactions::routes())
        .merge(library::routes())
        .merge(collaboration::routes())
        .merge(communications::routes())
        .merge(simulation::routes())
        .route("/health/live", get(hosting::live))
        .route("/health/ready", get(hosting::ready))
        .route(
            "/",
            get(|axum::extract::Query(params): axum::extract::Query<HashMap<String, String>>| async move {
                let flag = |name: &str| params.get(name).is_some_and(|value| value == "1");
                Redirect::to(if flag("admin") {
                    "/index.html?view=account&admin=1"
                } else if flag("choose") {
                    "/index.html?view=account&choose=1"
                } else { "/index.html?view=account" })
            }),
        )
        .route("/w/{workspace}/", get(workspaces::client))
        .route(
            "/w/{workspace}/api/{*rest}",
            axum::routing::any(workspaces::dispatch),
        )
        .route("/auth/login", get(routes::login))
        .route("/auth/callback", get(routes::callback))
        .route("/api/identity/session", get(routes::session))
        .route("/api/identity/logout", post(routes::logout))
        .route("/api/identity/profile", post(profile::update))
        .route("/api/identity/workspaces", post(routes::create))
        .route("/api/identity/workspace-codes/check", post(workspace_codes::check))
        .route("/api/identity/admin/workspace-codes", get(workspace_codes::list).post(workspace_codes::issue))
        .route("/api/identity/admin/workspace-codes/{id}/revoke", post(workspace_codes::revoke))
        .route("/api/identity/workspaces/{id}/people", get(routes::people))
        .route("/api/identity/workspaces/{id}/settings", get(settings::read).post(settings::update))
        .route("/api/identity/workspaces/{id}/activity", get(settings::activity))
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
            "/api/identity/invitations/{id}/decline",
            post(routes::decline),
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
    let started = std::time::Instant::now();
    let request_id = uuid::Uuid::new_v4().to_string();
    let headers = request.headers();
    let library_head = request.method() == axum::http::Method::HEAD
        && request
            .uri()
            .path()
            .starts_with("/api/identity/workspaces/")
        && request.uri().path().contains("/library/")
        && request.uri().path().ends_with("/download");
    let health = request.method() == axum::http::Method::GET
        && matches!(request.uri().path(), "/health/live" | "/health/ready");
    let host = app
        .config
        .origin
        .split_once("://")
        .map(|(_, host)| host)
        .unwrap_or("");
    if !health && headers.get(header::HOST).and_then(|v| v.to_str().ok()) != Some(host) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if request.method() != axum::http::Method::GET
        && !library_head
        && !(request.method() == axum::http::Method::POST
            && events::routes::webhook_path(request.uri().path()))
        && (headers.get(header::ORIGIN).and_then(|v| v.to_str().ok())
            != Some(app.config.origin.as_str())
            || headers.get("x-ayati-request").and_then(|v| v.to_str().ok()) != Some("identity"))
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    let path = request.uri().path();
    // No fallback into old local-owner APIs, even for authenticated users.
    if path.starts_with("/api/") && !path.starts_with("/api/identity/") {
        return StatusCode::NOT_FOUND.into_response();
    }
    let guide_document =
        path.starts_with("/w/") && request.uri().query() == Some("agent=local-agent&guide=1");
    let cache_asset = cacheable_asset(request.method(), path);
    let scoped_api = path.starts_with("/w/") && path.contains("/api/");
    let library_preview = path.starts_with("/api/identity/workspaces/")
        && path.contains("/library/")
        && path.ends_with("/download");
    let mut response = next.run(request).await;
    let response_status = response.status();
    let h = response.headers_mut();
    h.insert("x-relay-request-id", request_id.parse().unwrap());
    h.insert(
        "server-timing",
        format!("app;dur={:.3}", started.elapsed().as_secs_f64() * 1000.0)
            .parse()
            .unwrap(),
    );
    h.insert(
        header::CACHE_CONTROL,
        if cache_asset && response_status.is_success() {
            "private, max-age=31536000, immutable"
        } else {
            "no-store"
        }
        .parse()
        .unwrap(),
    );
    h.insert("referrer-policy", "no-referrer".parse().unwrap());
    h.insert("x-content-type-options", "nosniff".parse().unwrap());
    if scoped_api {
        return response;
    }
    if library_preview {
        h.insert(
            "content-security-policy",
            "sandbox; default-src 'none'; frame-ancestors 'self'"
                .parse()
                .unwrap(),
        );
        return response;
    }
    if guide_document {
        h.insert("content-security-policy", "default-src 'self'; connect-src 'self'; img-src 'self' data: blob:; media-src 'self' blob:; style-src 'self'; script-src 'self'; object-src 'none'; frame-ancestors 'self'; base-uri 'none'; form-action 'self'".parse().unwrap());
        return response;
    }
    h.insert("content-security-policy","default-src 'self'; connect-src 'self'; img-src 'self' data: blob:; media-src 'self' blob:; style-src 'self'; script-src 'self'; object-src 'none'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'".parse().unwrap());
    response
}

#[cfg(test)]
mod tests;

fn check(condition: bool, message: &str) -> anyhow::Result<()> {
    anyhow::ensure!(condition, "{message}");
    Ok(())
}

// Only Vite's content-addressed static code is reusable. Documents, API responses,
// App files and unsuccessful responses must never inherit this cache policy.
fn cacheable_asset(method: &axum::http::Method, path: &str) -> bool {
    if *method != axum::http::Method::GET && *method != axum::http::Method::HEAD {
        return false;
    }
    let Some(name) = path
        .strip_prefix("/assets/")
        .filter(|name| !name.contains('/'))
    else {
        return false;
    };
    let Some((stem, extension)) = name.rsplit_once('.') else {
        return false;
    };
    let bytes = stem.as_bytes();
    matches!(extension, "js" | "mjs" | "css")
        && bytes.len() > 9
        && bytes[bytes.len() - 9] == b'-'
        && bytes[bytes.len() - 8..]
            .iter()
            .all(|c| c.is_ascii_alphanumeric() || *c == b'_' || *c == b'-')
}

//! Authenticated workspace gateway. Legacy APIs remain inaccessible without workspace scope.
pub mod adoption;
mod agents;
pub mod assignment_launcher;
pub mod assignment_worker;
mod chat;
mod collaboration;
pub(crate) mod control;
mod controlled_launcher;
pub mod controlled_worker;
mod database;
pub mod executor;
mod hosting;
mod interactions;
mod library;
mod login_state;
mod membership_schema;
pub mod postgres;
mod profile;
mod provider;
mod resources;
mod routes;
mod settings;
pub mod simulation;
mod sqlite_access;
pub(crate) mod start_trace;
pub mod store;
pub mod worker;
mod worker_config;
mod worker_control;
pub mod worker_health;
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
    library: library::Storage,
    provider: Provider,
    store: database::Database,
    origin: String,
    pending: Mutex<HashMap<String, Pending>>,
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
        } else if error.is::<chat::store::QueueFull>()
            || error.is::<chat::store::BackgroundQueueFull>()
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
pub async fn serve(data: &Path, port: u16) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let hosting = hosting::Hosting::from_env(port)?;
    let provider = Provider::from_env()?;
    let listener = tokio::net::TcpListener::bind((hosting.bind, port))
        .await
        .context("Identity port is in use. Stop the other server or choose AYATI_PORT")?;
    std::fs::create_dir_all(data)?;
    let installation = data.canonicalize()?;
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
    let (store, bound) = if let Ok(url) = std::env::var("RELAY_DATABASE_URL") {
        let pg = postgres::PgStore::connect(&url).await?;
        pg.verify_schema().await?;
        if data.join("postgres-identity").exists()
            || adoption::has_identity_data(&data.join("identity.sqlite"))?
        {
            pg.verify_import().await?;
        }
        let bound = adoption::read_binding(&data.join("identity.sqlite"))?;
        if let Some((workspace, owner)) = &bound {
            anyhow::ensure!(
                pg.role(workspace, owner).await? == "Owner",
                "PostgreSQL does not contain this installation's owner; import identity first"
            );
        }
        (database::Database::Postgres(pg), bound)
    } else {
        anyhow::ensure!(
            !data.join("postgres-identity").exists(),
            "Identity was migrated; configure RELAY_DATABASE_URL to avoid stale SQLite data"
        );
        let store = Store::open(&data.join("identity.sqlite"))?;
        let bound = adoption::binding(&store)?;
        (store.into(), bound)
    };
    anyhow::ensure!(
        !hosting.origin.starts_with("https://") || bound.is_none(),
        "Hosted identity requires a fresh data directory; local installation bindings are not portable"
    );
    // The bound installation must never be opened simultaneously by the old daemon.
    let _installation_lock = if bound.is_some() {
        Some(adoption::lock(&installation.join("daemon.lock"))?)
    } else {
        None
    };
    let mut runtimes = workspaces::Runtimes::new(data.join("workspaces"));
    runtimes.bound = bound;
    let app = Arc::new(App {
        library: library::Storage::from_env()?,
        provider,
        store,
        origin: hosting.origin,
        pending: Mutex::new(HashMap::new()),
        runtimes,
        simulation: None,
    });
    tracing::info!("Identity workspace: {}/?view=account", app.origin);
    let control = worker_control::start(app.clone()).await?;
    chat::start_dispatcher(app.clone())?;
    let stopping = app.runtimes.stopping.clone();
    axum::serve(listener, router(app.clone()))
        .with_graceful_shutdown(async move {
            tokio::select! { _ = worker::shutdown_signal() => {}, _ = stopping.cancelled() => {} }
            stopping.cancel();
        })
        .await?;
    app.runtimes.shutdown().await?;
    if let Some(control) = control {
        control.await?;
    }
    Ok(())
}
fn router(app: Shared) -> Router {
    Router::new()
        .merge(agents::routes())
        .merge(control::routes())
        .merge(chat::routes())
        .merge(interactions::routes())
        .merge(library::routes())
        .merge(collaboration::routes())
        .merge(simulation::routes())
        .merge(resources::routes())
        .route("/health/live", get(hosting::live))
        .route("/health/ready", get(hosting::ready))
        .route(
            "/",
            get(|axum::extract::Query(params): axum::extract::Query<HashMap<String, String>>| async move {
                Redirect::to(if params.get("choose").is_some_and(|value| value == "1") {
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
        .origin
        .split_once("://")
        .map(|(_, host)| host)
        .unwrap_or("");
    if !health && headers.get(header::HOST).and_then(|v| v.to_str().ok()) != Some(host) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if request.method() != axum::http::Method::GET
        && !library_head
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

#[cfg(test)]
mod hosting_tests;
#[cfg(test)]
mod ownership_tests;

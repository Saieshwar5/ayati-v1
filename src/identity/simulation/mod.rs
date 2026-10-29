//! Explicit, isolated local company rehearsal. Never selected by normal identity mode.
pub(in crate::identity) mod dispatch;
mod executor;
mod reports;
mod seed;
use super::*;
use axum::{Json, extract::Path as RoutePath, http::HeaderMap};
pub(super) use executor::execute;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;
#[derive(Serialize, Deserialize, Clone)]
pub struct Employee {
    pub number: usize,
    pub name: String,
    pub team: String,
    pub user: User,
}
pub struct Simulation {
    pub employees: Vec<Employee>,
    pub workspace: String,
    pub root: PathBuf,
    pub paused: std::sync::atomic::AtomicBool,
}
pub async fn serve(root: &Path, config: crate::config::Controller) -> Result<()> {
    let port = config.common.port;
    let parsed = reqwest::Url::parse(&config.database_url)?;
    check(
        parsed
            .path()
            .trim_start_matches('/')
            .starts_with("relay_simulation_"),
        "Simulation database name must start with relay_simulation_",
    )?;
    check(
        matches!(parsed.host_str(), Some("localhost" | "127.0.0.1")),
        "Simulation requires a local PostgreSQL server",
    )?;
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await?;
    if root.exists() && !root.join("SIMULATION").exists() {
        check(
            std::fs::read_dir(root)?.next().is_none(),
            "Refusing an existing non-simulation data directory",
        )?;
    }
    std::fs::create_dir_all(root)?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o700))?;
    let lock = process_lock::exclusive(&root.join("server.lock"))?;
    let root = root.canonicalize()?;
    let marker = root.join("SIMULATION");
    let database_name = parsed.path();
    if marker.exists() {
        check(
            std::fs::read_to_string(&marker)? == database_name,
            "This simulation directory belongs to another database",
        )?;
    }
    let store = postgres::PgStore::open(&config).await?;
    super::configure_process(&config);
    std::fs::write(&marker, database_name)?;
    let simulation = seed::prepare(&root, &store).await?;
    let app = Arc::new(App {
        library: library::Storage::from_settings(&config.library)?,
        provider: Provider::simulation(),
        store,
        config: Arc::new(config),
        runtimes: workspaces::Runtimes::new(root.join("workspaces")),
        simulation: Some(simulation),
    });
    let control = worker_control::start(app.clone()).await?;
    chat::start_dispatcher(app.clone());
    rehearsals::start(app.clone()).await;
    automations::start(app.clone()).await;
    tracing::info!(
        "Company simulation: {}/index.html?view=simulation",
        app.config.origin
    );
    let result = super::shutdown::serve(app, listener, control).await;
    drop(lock);
    result
}
pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route("/api/identity/simulation", get(info))
        .route("/api/identity/simulation/reports", get(reports::reports))
        .route("/api/identity/simulation/dispatch", post(dispatch))
        .route(
            "/api/identity/simulation/employees/{number}/login",
            post(login),
        )
}
async fn info(State(app): State<Shared>) -> ApiResult<Json<Value>> {
    let sim = app.simulation.as_ref().ok_or_else(|| {
        Error(
            anyhow::anyhow!("Simulation is disabled"),
            StatusCode::NOT_FOUND,
        )
    })?;
    Ok(Json(
        json!({"workspace":sim.workspace,"employees":sim.employees,"mode":"scripted, no inference or external services"}),
    ))
}
async fn login(
    State(app): State<Shared>,
    RoutePath(number): RoutePath<usize>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let sim = app.simulation.as_ref().context("Simulation is disabled")?;
    let employee = sim
        .employees
        .iter()
        .find(|e| e.number == number)
        .context("Unknown employee")?;
    if let Some(old) = cookie(&headers, SESSION) {
        app.store.logout(&digest(&old)).await?;
    }
    let token = random_token();
    app.store
        .login(
            &format!("simulation_employee_{number}"),
            &employee.user.email,
            &digest(&token),
            "local-simulation",
        )
        .await?;
    let mut response =
        Json(json!({"workspace":sim.workspace,"user":employee.user})).into_response();
    response
        .headers_mut()
        .append(header::SET_COOKIE, set_cookie(&app, SESSION, &token, 3600));
    Ok(response)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Dispatch {
    enabled: bool,
}
async fn dispatch(
    State(app): State<Shared>,
    headers: HeaderMap,
    Json(input): Json<Dispatch>,
) -> ApiResult<Json<Value>> {
    let sim = app.simulation.as_ref().context("Simulation is disabled")?;
    let user = identity(&app, &headers).await?;
    check(
        app.store.role(&sim.workspace, &user.id).await? == "Owner",
        "Only the simulation owner may pause dispatch",
    )?;
    sim.paused
        .store(!input.enabled, std::sync::atomic::Ordering::Release);
    if input.enabled {
        app.runtimes.chat_wake.notify_one();
    }
    Ok(Json(json!({"enabled":input.enabled})))
}

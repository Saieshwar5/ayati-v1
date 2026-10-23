//! Separate, opt-in local machine API. Browser sessions and origins confer no access.
pub(crate) mod attempt;
pub(crate) mod client;
pub(crate) mod config;
mod hub;
pub(crate) mod model;
mod pool;
use super::{Shared, chat::execution, database::Database, postgres::PgStore, provider::digest};
use anyhow::{Context, Result, ensure};
use axum::{
    Json, Router,
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, Request, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::post,
};
use model::*;
use serde_json::{Value, json};
use std::sync::Arc;

struct Control {
    app: Shared,
    bootstrap_hash: String,
    host: String,
    lease: u64,
    capacity: usize,
    hub: std::sync::Arc<hub::Hub>,
}
type StateData = Arc<Control>;
struct Error(anyhow::Error);
type Api<T> = std::result::Result<Json<T>, Error>;
impl<E: Into<anyhow::Error>> From<E> for Error {
    fn from(error: E) -> Self {
        Self(error.into())
    }
}
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let (status, message) = if self
            .0
            .is::<super::postgres::worker_control::ControlDenied>()
        {
            (
                StatusCode::FORBIDDEN,
                "Machine credential or assignment scope denied",
            )
        } else if self.0.is::<sea_orm::DbErr>() {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "Worker control temporarily unavailable",
            )
        } else {
            (
                StatusCode::CONFLICT,
                "Assignment unavailable or request conflicts with its current state",
            )
        };
        (status, Json(json!({"error":message}))).into_response()
    }
}
fn pg(control: &Control) -> &PgStore {
    let Database::Postgres(pg) = &control.app.store else {
        unreachable!()
    };
    pg
}
fn credential(headers: &HeaderMap) -> Result<String> {
    ensure!(
        headers.get_all(header::AUTHORIZATION).iter().count() == 1,
        "One machine credential required"
    );
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .context("Machine credential required")?;
    model::token(token)?;
    Ok(digest(token))
}
pub(super) async fn start(app: Shared) -> Result<Option<tokio::task::JoinHandle<()>>> {
    let origin = std::env::var("AYATI_WORKER_CONTROL_URL").ok();
    let key = std::env::var_os("AYATI_WORKER_CONTROL_KEY_FILE");
    if origin.is_none() && key.is_none() {
        ensure!(
            !matches!(&app.store,Database::Postgres(pg) if pg.pooled),
            "Pooled execution requires the worker-control listener"
        );
        return Ok(None);
    }
    let origin = config::url(&origin.context("Worker control requires AYATI_WORKER_CONTROL_URL")?)?;
    let host = origin.strip_prefix("http://").unwrap().to_string();
    let address = config::address(&host)?;
    ensure!(
        matches!(app.store, Database::Postgres(_))
            && matches!(
                super::worker_config::Mode::from_env(true)?,
                super::worker_config::Mode::Assigned | super::worker_config::Mode::Pooled
            ),
        "Worker control requires PostgreSQL and assigned execution"
    );
    let key = config::key(std::path::Path::new(
        &key.context("Worker control requires AYATI_WORKER_CONTROL_KEY_FILE")?,
    ))?;
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .context("Worker control port unavailable")?;
    let state = Arc::new(Control {
        app: app.clone(),
        bootstrap_hash: digest(&key),
        host,
        lease: super::worker_config::seconds("AYATI_ASSIGNMENT_LEASE_SECONDS", 120, 6, 300)?,
        capacity: super::worker_config::seconds("AYATI_ASSIGNMENT_CAPACITY", 2, 1, 16)? as usize,
        hub: std::sync::Arc::new(hub::Hub::default()),
    });
    let router = routes(state.clone());
    let shutdown_hub = state.hub.clone();
    let coordinator = if pg(&state).pooled {
        let capacity = super::worker_config::seconds(
            "AYATI_EXECUTOR_CAPACITY",
            state.capacity as u64,
            1,
            1024,
        )? as usize;
        let limits = super::postgres::PoolLimits {
            capacity,
            lease: state.lease,
            workspace: super::worker_config::seconds(
                "AYATI_EXECUTOR_WORKSPACE_LIMIT",
                capacity as u64,
                1,
                1024,
            )? as usize,
            model: super::worker_config::seconds(
                "AYATI_EXECUTOR_MODEL_LIMIT",
                capacity as u64,
                1,
                1024,
            )? as usize,
        };
        let expiry = super::worker_config::seconds("AYATI_EXECUTOR_EXPIRY_SECONDS", 15, 6, 120)?;
        Some(tokio::spawn(pool::dispatch(state, limits, expiry)))
    } else {
        None
    };
    let stopping = app.runtimes.stopping.clone();
    Ok(Some(tokio::spawn(async move {
        let stopped = stopping.clone();
        if axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                stopped.cancelled().await;
                shutdown_hub.close();
            })
            .await
            .is_err()
        {
            tracing::error!("Local worker control server stopped unexpectedly");
            stopping.cancel();
        }
        if let Some(coordinator) = coordinator {
            let _ = coordinator.await;
        }
    })))
}
fn routes(state: StateData) -> Router {
    Router::new()
        .merge(pool::routes())
        .route("/v1/executors/register", post(register))
        .route(
            "/v1/executors/{executor}/{incarnation}/reserve",
            post(reserve),
        )
        .route(
            "/v1/executors/{executor}/{incarnation}/release",
            post(release),
        )
        .route("/v1/assignments/{assignment}/start", post(start_assignment))
        .route("/v1/assignments/{assignment}/renew", post(renew))
        .route("/v1/assignments/{assignment}/complete", post(complete))
        .layer(axum::extract::DefaultBodyLimit::max(512 * 1024))
        .layer(middleware::from_fn_with_state(state.clone(), guard))
        .with_state(state)
}
async fn guard(State(state): State<StateData>, request: Request<Body>, next: Next) -> Response {
    let headers = request.headers();
    let mut response = if headers.get(header::HOST).and_then(|h| h.to_str().ok())
        != Some(&state.host)
        || headers.contains_key(header::ORIGIN)
        || headers.contains_key(header::COOKIE)
        || request.uri().query().is_some()
    {
        StatusCode::FORBIDDEN.into_response()
    } else if credential(headers).is_err() {
        StatusCode::UNAUTHORIZED.into_response()
    } else {
        next.run(request).await
    };
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response.headers_mut().insert(
        "x-relay-request-id",
        uuid::Uuid::new_v4().to_string().parse().unwrap(),
    );
    response
}
async fn register(
    State(state): State<StateData>,
    headers: HeaderMap,
    Json(input): Json<Registration>,
) -> Api<Value> {
    super::postgres::worker_control::require(credential(&headers)? == state.bootstrap_hash)?;
    super::postgres::worker_control::require(input.pooled == pg(&state).pooled)?;
    pg(&state).register_executor(&input).await?;
    Ok(Json(json!({"registered":true})))
}
async fn reserve(
    State(state): State<StateData>,
    Path((executor, incarnation)): Path<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<Reservation>,
) -> Api<Option<Scope>> {
    super::postgres::worker_control::require(!pg(&state).pooled)?;
    pg(&state)
        .executor_allowed(&executor, &incarnation, &credential(&headers)?)
        .await?;
    Ok(Json(
        pg(&state)
            .reserve_controlled(&executor, &incarnation, &input, state.lease, state.capacity)
            .await?,
    ))
}
async fn release(
    State(state): State<StateData>,
    Path((executor, incarnation)): Path<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<Release>,
) -> Api<Value> {
    pg(&state)
        .executor_allowed(&executor, &incarnation, &credential(&headers)?)
        .await?;
    pg(&state)
        .release_controlled(&executor, &incarnation, &input.scope, &input.reason)
        .await?;
    Ok(Json(json!({"released":true})))
}
async fn start_assignment(
    State(state): State<StateData>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(input): Json<AssignedRequest>,
) -> Api<Value> {
    super::postgres::worker_control::require(id == input.scope.assignment)?;
    model::uuid(&input.session)?;
    let hash = credential(&headers)?;
    let record = pg(&state)
        .controlled_assignment(&input.scope, &hash)
        .await?;
    let context = if let Some(context) = record.context {
        serde_json::from_value(context)?
    } else {
        let profile = execution::managed_profile(&state.app.store, &record.assignment.job)
            .await?
            .context("Agent unavailable")?;
        execution::prepare(
            &state.app.store,
            &record.assignment.job,
            profile,
            record.assignment.run_seconds,
        )
        .await?
    };
    let context = pg(&state).start_controlled(&input, &hash, &context).await?;
    // Replaying a lost start response must return a newly confirmed full lease,
    // rather than presenting a nearly expired original lease as fresh.
    let lease = pg(&state).renew_controlled(&input, &hash).await?;
    Ok(Json(json!({"context":context,"lease_seconds":lease})))
}
async fn renew(
    State(state): State<StateData>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(input): Json<AssignedRequest>,
) -> Api<Value> {
    super::postgres::worker_control::require(id == input.scope.assignment)?;
    let lease = pg(&state)
        .renew_controlled(&input, &credential(&headers)?)
        .await?;
    Ok(Json(json!({"lease_seconds":lease})))
}
async fn complete(
    State(state): State<StateData>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(input): Json<Completion>,
) -> Api<Value> {
    super::postgres::worker_control::require(id == input.assignment.scope.assignment)?;
    let result = pg(&state)
        .complete_controlled(&input, &credential(&headers)?)
        .await?;
    Ok(Json(json!({"state":result})))
}

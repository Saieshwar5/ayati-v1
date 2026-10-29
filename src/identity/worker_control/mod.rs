//! Separate, opt-in local machine API. Browser sessions and origins confer no access.
pub(crate) mod attempt;
pub(crate) mod client;
#[cfg(test)]
mod client_tests;
pub(crate) mod cloud;
pub(crate) mod config;
mod hub;
pub(crate) mod model;
pub(crate) mod output;
mod pool;
mod tools;
use super::{
    Shared,
    chat::execution,
    postgres::{PgStore, worker_control::Work},
    provider::digest,
};
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
    &control.app.store
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
    // Config validation already required this for pooled execution.
    let Some(settings) = app.config.worker_control.clone() else {
        return Ok(None);
    };
    let origin = config::url(&settings.url)?;
    let host = origin.strip_prefix("http://").unwrap().to_string();
    let address = config::address(&host)?;
    ensure!(
        app.store.pooled,
        "Worker control requires PostgreSQL and pooled execution"
    );
    let key = config::key(&settings.key_file)?;
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .context("Worker control port unavailable")?;
    let pool = app.config.pool.clone();
    let state = Arc::new(Control {
        app: app.clone(),
        bootstrap_hash: digest(&key),
        host,
        hub: std::sync::Arc::new(hub::Hub::default()),
    });
    let router = routes(state.clone());
    let shutdown_hub = state.hub.clone();
    let limits = super::postgres::PoolLimits {
        capacity: pool.capacity,
        lease: pool.lease_seconds,
        chat_reserve: pool.chat_reserve,
        workspace: pool.workspace_limit,
        model: pool.model_limit,
    };
    let coordinator = tokio::spawn(pool::dispatch(state, limits, pool.expiry_seconds));
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
        let _ = coordinator.await;
    })))
}
fn routes(state: StateData) -> Router {
    Router::new()
        .merge(pool::routes())
        .route("/v1/assignments/{id}/output", post(output::append))
        .route("/v1/assignments/{id}/tools", post(tools::invoke))
        .route("/v1/executors/register", post(register))
        .route("/v1/executors/register-cloud", post(cloud::register))
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
    } else if !matches!(
        request.uri().path(),
        "/v1/executors/register" | "/v1/executors/register-cloud"
    ) && pg(&state)
        .cloud_credential_allowed(
            &credential(headers).unwrap(),
            headers
                .get("x-ayati-cloud-task")
                .and_then(|h| h.to_str().ok()),
        )
        .await
        .is_err()
    {
        StatusCode::FORBIDDEN.into_response()
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
async fn start_assignment(
    State(state): State<StateData>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(input): Json<AssignedRequest>,
) -> Api<Value> {
    let mut trace = super::start_trace::StartTrace::new();
    super::postgres::worker_control::require(id == input.scope.assignment)?;
    model::uuid(&input.session)?;
    model::uuid(&input.scope.assignment)?;
    let hash = credential(&headers)?;
    super::start_trace::point(&input.scope.assignment, "controller", "start_handler_enter");
    let record = pg(&state)
        .controlled_assignment(&input.scope, &hash)
        .await?;
    trace.mark("scope_lookup");
    let context = match (record.context, &record.work) {
        (Some(context), _) => serde_json::from_value(context)?,
        (None, Work::Reply(assignment)) => {
            execution::prepare_traced(
                &state.app.store,
                &assignment.job,
                assignment.run_seconds,
                &mut trace,
            )
            .await?
        }
        (None, Work::Task(task)) => pg(&state).task_context(task).await?,
    };
    let context = pg(&state).start_controlled(&input, &hash, &context).await?;
    trace.mark("start_commit");
    // The assignment's immutable lease duration was confirmed atomically with
    // context/session ownership, including on an exact start replay.
    let lease = record.lease_seconds;
    trace.finish(&input.scope.assignment, "controller");
    super::start_trace::point(
        &input.scope.assignment,
        "controller",
        "start_handler_return",
    );
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

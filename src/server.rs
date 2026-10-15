use crate::{agent::Agent, files::MAX_FILE_BYTES};
use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, Multipart, Path, State},
    http::{HeaderMap, Request, StatusCode, header},
    middleware::{self, Next},
    response::{
        IntoResponse, Response, Sse,
        sse::{Event, KeepAlive},
    },
    routing::{get, post},
};
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{convert::Infallible, sync::Arc};
use tokio_stream::wrappers::BroadcastStream;
use tokio_util::sync::CancellationToken;
use tower_http::services::ServeDir;

#[derive(Clone)]
pub struct WebState {
    pub agent: Arc<Agent>,
    pub model_ready: bool,
    pub model_name: String,
    pub port: u16,
    pub shutdown: CancellationToken,
}

type ApiResult<T> = Result<T, ApiError>;
struct ApiError(anyhow::Error);
impl<E: Into<anyhow::Error>> From<E> for ApiError {
    fn from(e: E) -> Self {
        Self(e.into())
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("{:#}", self.0) })),
        )
            .into_response()
    }
}

pub fn router(state: WebState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/state", get(snapshot))
        .route("/api/messages", post(message))
        .route("/api/tasks/{id}/stop", post(stop))
        .route("/api/events", get(events))
        .route("/api/files", post(upload))
        .route("/api/files/{id}", get(download))
        .route("/api/shutdown", post(shutdown))
        .fallback_service(ServeDir::new("web/dist").append_index_html_on_directories(true))
        .layer(DefaultBodyLimit::max(MAX_FILE_BYTES as usize + 64 * 1024))
        .layer(middleware::from_fn_with_state(state.clone(), local_request))
        .with_state(state)
}

async fn local_request(
    State(state): State<WebState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let headers = request.headers();
    // A local daemon still needs protection against websites posting to localhost.
    let hosts = [
        format!("127.0.0.1:{}", state.port),
        format!("localhost:{}", state.port),
    ];
    let host_ok = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|host| hosts.iter().any(|allowed| host == allowed));
    let origin_ok = headers
        .get(header::ORIGIN)
        .map(|v| {
            v.to_str()
                .ok()
                .is_some_and(|origin| hosts.iter().any(|host| origin == format!("http://{host}")))
        })
        .unwrap_or(true);
    let cross_site =
        headers.get("sec-fetch-site").and_then(|v| v.to_str().ok()) == Some("cross-site");
    if !host_ok || !origin_ok || cross_site {
        return (
            StatusCode::FORBIDDEN,
            "Only local same-origin requests are accepted",
        )
            .into_response();
    }
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert("x-content-type-options", "nosniff".parse().unwrap());
    headers.insert("referrer-policy", "no-referrer".parse().unwrap());
    headers.insert("content-security-policy", "default-src 'self'; connect-src 'self'; img-src 'self' data:; style-src 'self'; script-src 'self'; object-src 'none'; frame-ancestors 'none'; base-uri 'none'".parse().unwrap());
    response
}

async fn health(State(state): State<WebState>) -> Json<Value> {
    Json(json!({ "status": "ready", "model_ready": state.model_ready,
        "model": state.model_name, "version": env!("CARGO_PKG_VERSION") }))
}

async fn snapshot(State(state): State<WebState>) -> ApiResult<Json<Value>> {
    Ok(Json(state.agent.snapshot()?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewMessage {
    content: String,
}
async fn message(
    State(state): State<WebState>,
    Json(input): Json<NewMessage>,
) -> ApiResult<Json<Value>> {
    if !state.model_ready {
        return Err(anyhow::anyhow!("Configure FIREWORKS_API_KEY in your local .env and restart Ayati before sending a request").into());
    }
    Ok(Json(json!(state.agent.submit(&input.content)?)))
}

async fn stop(State(state): State<WebState>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    state.agent.stop(&id)?;
    Ok(Json(json!({ "ok": true })))
}

async fn events(
    State(state): State<WebState>,
) -> Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>> {
    let stream = BroadcastStream::new(state.agent.events.subscribe()).map(|result| {
        let data = match result {
            Ok(event) => serde_json::to_string(&event).unwrap_or_default(),
            Err(_) => json!({ "kind": "resync" }).to_string(),
        };
        Ok(Event::default().data(data))
    });
    Sse::new(stream.take_until(state.shutdown.cancelled_owned())).keep_alive(KeepAlive::default())
}

async fn upload(State(state): State<WebState>, mut multipart: Multipart) -> ApiResult<Json<Value>> {
    let field = multipart
        .next_field()
        .await?
        .ok_or_else(|| anyhow::anyhow!("Choose a file"))?;
    let name = field.file_name().unwrap_or("attachment").to_string();
    let bytes = field.bytes().await?;
    let (artifact, workspace_path) = state.agent.files.upload(&name, &bytes).await?;
    Ok(Json(
        json!({ "artifact": artifact, "workspace_path": workspace_path }),
    ))
}

async fn download(State(state): State<WebState>, Path(id): Path<String>) -> ApiResult<Response> {
    let artifact = state
        .agent
        .store
        .artifact(&id)?
        .ok_or_else(|| anyhow::anyhow!("File does not exist"))?;
    let file =
        tokio::fs::File::open(state.agent.files.archives.join(&artifact.stored_name)).await?;
    let stream = tokio_util::io::ReaderStream::new(file);
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "application/octet-stream".parse()?);
    // Stored filenames may come from shell output. Use a fixed ASCII fallback and encoded UTF-8.
    let encoded: String = artifact.name.bytes().map(|b| format!("%{b:02X}")).collect();
    headers.insert(
        header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"download\"; filename*=UTF-8''{encoded}").parse()?,
    );
    headers.insert(header::CACHE_CONTROL, "no-store".parse()?);
    Ok((headers, Body::from_stream(stream)).into_response())
}

async fn shutdown(State(state): State<WebState>) -> Json<Value> {
    state.agent.stop_all();
    state.shutdown.cancel();
    Json(json!({ "ok": true }))
}

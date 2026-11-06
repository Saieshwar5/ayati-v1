//! The executor slot's loopback model endpoint in gateway mode (core-agent CA3).
//! Goose speaks OpenAI's protocol to it; each request is forwarded, with the current
//! assignment's capability, to the controller, which alone holds provider keys. Between
//! assignments it is disarmed and refuses every request.
use crate::identity::control::broker::Access;
use anyhow::Result;
use axum::{
    Router,
    body::{Body, Bytes},
    extract::State,
    http::{HeaderMap, Method, StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct ProxyState {
    key: String,
    host: String,
    armed: Arc<Mutex<Option<Access>>>,
}
pub(super) struct ModelProxy {
    task: tokio::task::JoinHandle<()>,
    pub settings: crate::config::ModelProxy,
    armed: Arc<Mutex<Option<Access>>>,
}
impl Drop for ModelProxy {
    fn drop(&mut self) {
        self.task.abort();
    }
}
/// While held, the proxy forwards for one assignment; dropping it disarms the proxy.
pub(super) struct Armed(Arc<Mutex<Option<Access>>>);
impl Drop for Armed {
    fn drop(&mut self) {
        *self.0.lock().unwrap() = None;
    }
}
impl ModelProxy {
    pub async fn start() -> Result<Self> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let host = listener.local_addr()?.to_string();
        let key = crate::identity::provider::random_token();
        let armed = Arc::new(Mutex::new(None));
        let state = ProxyState {
            key: key.clone(),
            host: host.clone(),
            armed: armed.clone(),
        };
        let router = Router::new()
            .fallback(forward)
            .layer(axum::extract::DefaultBodyLimit::max(
                super::super::worker_control::MODEL_BODY_LIMIT,
            ))
            .with_state(state);
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        Ok(Self {
            task,
            settings: crate::config::ModelProxy {
                host: format!("http://{host}"),
                key,
            },
            armed,
        })
    }
    pub fn arm(&self, access: Access) -> Armed {
        *self.armed.lock().unwrap() = Some(access);
        Armed(self.armed.clone())
    }
}

fn refuse(status: StatusCode, message: &str) -> Response {
    (status, axum::Json(json!({"error":{"message":message}}))).into_response()
}

async fn forward(
    State(state): State<ProxyState>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "));
    if bearer != Some(state.key.as_str())
        || headers.get(header::HOST).and_then(|h| h.to_str().ok()) != Some(&state.host)
        || headers.contains_key(header::ORIGIN)
        || headers.contains_key(header::COOKIE)
    {
        return refuse(StatusCode::FORBIDDEN, "Model proxy access denied");
    }
    if method == Method::GET && uri.path().ends_with("/models") {
        return axum::Json(json!({"object":"list","data":[]})).into_response();
    }
    if method != Method::POST || !uri.path().ends_with("/chat/completions") {
        return refuse(
            StatusCode::NOT_FOUND,
            "Only chat completions go through the model gateway",
        );
    }
    let Some(access) = state.armed.lock().unwrap().clone() else {
        return refuse(
            StatusCode::SERVICE_UNAVAILABLE,
            "No assignment is running in this slot",
        );
    };
    let Ok(request) = serde_json::from_slice::<Value>(&body) else {
        return refuse(StatusCode::BAD_REQUEST, "Invalid model request");
    };
    let path = format!(
        "/v1/assignments/{}/model",
        access.assignment.scope.assignment
    );
    let envelope = json!({"assignment": access.assignment, "body": request});
    match access.client.post_stream(&path, &envelope, 600).await {
        Ok(response) => {
            let status = response.status();
            let kind = response.headers().get(header::CONTENT_TYPE).cloned();
            let mut built = Response::builder().status(status.as_u16());
            if let Some(kind) = kind {
                built = built.header(header::CONTENT_TYPE, kind);
            }
            built
                .body(Body::from_stream(response.bytes_stream()))
                .unwrap_or_else(|_| {
                    refuse(StatusCode::BAD_GATEWAY, "Model gateway response failed")
                })
        }
        Err(_) => refuse(StatusCode::BAD_GATEWAY, "The model gateway is unavailable"),
    }
}

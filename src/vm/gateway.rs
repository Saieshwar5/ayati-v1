use anyhow::{Context, Result, ensure};
use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

struct Provider {
    url: reqwest::Url,
    key: String,
    token: String,
    model: String,
    client: reqwest::Client,
    failed: Arc<AtomicBool>,
}

pub struct ModelGateway {
    pub port: u16,
    pub token: String,
    pub failed: Arc<AtomicBool>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for ModelGateway {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl ModelGateway {
    pub async fn start_with_memory(
        provider: &str,
        model: &str,
        memory: Option<Arc<crate::memory::MemoryAccess>>,
    ) -> Result<Self> {
        let (url, key_name) = match provider {
            "fireworks-ai" => (
                "https://api.fireworks.ai/inference/v1/chat/completions".into(),
                "FIREWORKS_API_KEY",
            ),
            "openai" => {
                let host = std::env::var("OPENAI_HOST")
                    .unwrap_or_else(|_| "https://api.openai.com".into());
                let base = std::env::var("OPENAI_BASE_PATH")
                    .unwrap_or_else(|_| "v1/chat/completions".into());
                (
                    format!("{}/{}", host.trim_end_matches('/'), base.trim_matches('/')),
                    "OPENAI_API_KEY",
                )
            }
            _ => anyhow::bail!("VM proof supports only the configured OpenAI-compatible providers"),
        };
        let url = reqwest::Url::parse(&url)?;
        ensure!(
            url.scheme() == "https"
                || (url.scheme() == "http"
                    && matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "::1"))),
            "Model endpoint must use HTTPS; HTTP fixtures must be loopback"
        );
        ensure!(
            url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none(),
            "Model endpoint cannot contain credentials or query parameters"
        );
        let key =
            std::env::var(key_name).context("Configure the selected model provider locally")?;
        ensure!(
            !key.trim().is_empty(),
            "Configure the selected model provider locally"
        );
        let token = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        let failed = Arc::new(AtomicBool::new(false));
        let provider = Arc::new(Provider {
            failed: failed.clone(),
            url,
            key,
            token: token.clone(),
            model: model.into(),
            client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(std::time::Duration::from_secs(120))
                .build()?,
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        let mut app = router(provider);
        if let Some(memory) = memory {
            app = app.merge(crate::memory::routes(memory));
        }
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        Ok(Self {
            port,
            token,
            task,
            failed,
        })
    }
}

fn router(provider: Arc<Provider>) -> Router {
    Router::new()
        .route("/v1/chat/completions", post(completion))
        .route("/v1/models", get(models))
        .layer(DefaultBodyLimit::max(4 * 1024 * 1024))
        .with_state(provider)
}

fn authorized(headers: &HeaderMap, provider: &Provider) -> bool {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v == format!("Bearer {}", provider.token))
}

async fn models(
    State(provider): State<Arc<Provider>>,
    headers: HeaderMap,
) -> axum::response::Response {
    if !authorized(&headers, &provider) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    Json(json!({"object":"list","data":[{"id":provider.model,"object":"model","owned_by":"configured-provider"}]})).into_response()
}

async fn completion(
    State(provider): State<Arc<Provider>>,
    headers: HeaderMap,
    Json(input): Json<Value>,
) -> axum::response::Response {
    if !authorized(&headers, &provider) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    provider.failed.store(false, Ordering::SeqCst);
    if input.get("model").and_then(Value::as_str) != Some(&provider.model) {
        provider.failed.store(true, Ordering::SeqCst);
        return StatusCode::FORBIDDEN.into_response();
    }
    let response = provider
        .client
        .post(provider.url.clone())
        .bearer_auth(&provider.key)
        .json(&input)
        .send()
        .await;
    let Ok(response) = response else {
        provider.failed.store(true, Ordering::SeqCst);
        return (StatusCode::BAD_GATEWAY, "Model service unavailable").into_response();
    };
    if !response.status().is_success() {
        provider.failed.store(true, Ordering::SeqCst);
        return (
            StatusCode::BAD_GATEWAY,
            "Model service rejected the request",
        )
            .into_response();
    }
    let kind = response.headers().get("content-type").cloned();
    let mut result = Body::from_stream(response.bytes_stream()).into_response();
    if let Some(kind) = kind {
        result.headers_mut().insert("content-type", kind);
    }
    result
}

#[cfg(test)]
mod tests;

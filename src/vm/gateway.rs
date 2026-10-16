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
    Arc, RwLock,
    atomic::{AtomicBool, Ordering},
};

struct ModelAccess {
    token: String,
    model: String,
}

struct Provider {
    url: reqwest::Url,
    key: String,
    access: Arc<RwLock<ModelAccess>>,
    client: reqwest::Client,
    failed: Arc<AtomicBool>,
}

pub struct ModelGateway {
    pub port: u16,
    access: Arc<RwLock<ModelAccess>>,
    pub failed: Arc<AtomicBool>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for ModelGateway {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl ModelGateway {
    // Called only while the agent lifecycle gate is held and no prompt is active.
    // Keep the guest-forwarded port, but revoke the previous model's credential.
    pub fn select_model(&self, model: &str) -> Result<String> {
        let mut access = self
            .access
            .write()
            .map_err(|_| anyhow::anyhow!("Model gateway unavailable"))?;
        if access.model != model {
            access.model = model.into();
            access.token = format!(
                "{}{}",
                uuid::Uuid::new_v4().simple(),
                uuid::Uuid::new_v4().simple()
            );
        }
        Ok(access.token.clone())
    }

    pub async fn start_with_memory(
        provider: &str,
        model: &str,
        memory: Option<Arc<crate::memory::MemoryAccess>>,
    ) -> Result<Self> {
        let (url, key_name) = crate::runtime::provider::endpoint(provider)?;
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
        let access = Arc::new(RwLock::new(ModelAccess {
            token,
            model: model.into(),
        }));
        let failed = Arc::new(AtomicBool::new(false));
        let provider = Arc::new(Provider {
            failed: failed.clone(),
            url,
            key,
            access: access.clone(),
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
            access,
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

fn authorized_model(headers: &HeaderMap, provider: &Provider) -> Option<String> {
    let access = provider.access.read().ok()?;
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .filter(|v| *v == format!("Bearer {}", access.token))
        .map(|_| access.model.clone())
}

async fn models(
    State(provider): State<Arc<Provider>>,
    headers: HeaderMap,
) -> axum::response::Response {
    let Some(model) = authorized_model(&headers, &provider) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    Json(json!({"object":"list","data":[{"id":model,"object":"model","owned_by":"configured-provider"}]})).into_response()
}

async fn completion(
    State(provider): State<Arc<Provider>>,
    headers: HeaderMap,
    Json(input): Json<Value>,
) -> axum::response::Response {
    let Some(model) = authorized_model(&headers, &provider) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    provider.failed.store(false, Ordering::SeqCst);
    if input.get("model").and_then(Value::as_str) != Some(model.as_str()) {
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

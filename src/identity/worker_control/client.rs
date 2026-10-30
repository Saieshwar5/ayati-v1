//! Bounded machine transport. Never follows redirects, uses proxies or prints credentials/bodies.
use super::config;
use anyhow::{Result, ensure};
use serde::{Serialize, de::DeserializeOwned};
use std::time::Duration;

/// A transient failure worth retrying. `unreachable` means nothing answered at
/// all (connection refused, name not resolvable yet), as while a controller
/// starts; a rejected TLS certificate or a bad response is never unreachable.
#[derive(Debug, Default)]
pub(crate) struct Unavailable {
    unreachable: bool,
}
impl Unavailable {
    fn from_send(error: reqwest::Error) -> Self {
        Self {
            unreachable: error.is_connect() && !tls_rejected(&error),
        }
    }
}
/// TLS failures arrive as nested `io::Error`s around a `rustls::Error`.
fn tls_rejected(error: &(dyn std::error::Error + 'static)) -> bool {
    let mut next = Some(error);
    while let Some(error) = next {
        if error.is::<rustls::Error>() {
            return true;
        }
        next = match error.downcast_ref::<std::io::Error>() {
            Some(io) => io.get_ref().map(|inner| inner as _),
            None => error.source(),
        };
    }
    false
}
impl std::fmt::Display for Unavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Controller temporarily unavailable")
    }
}
impl std::error::Error for Unavailable {}
#[derive(Debug)]
pub(crate) struct Conflict;
impl std::fmt::Display for Conflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Controller state has not admitted this transition")
    }
}
impl std::error::Error for Conflict {}
#[derive(Clone)]
pub(crate) struct Client {
    http: reqwest::Client,
    /// The offer stream stays open, so it has no overall request timeout.
    stream: reqwest::Client,
    url: String,
    token: String,
    /// A cloud executor's bound ECS task, sent with every request so the
    /// controller can match each capability to its registration.
    task: Option<String>,
}
/// Both clients share one rule: plain HTTP only to loopback, otherwise HTTPS
/// trusting only the operator's private certificate authority.
fn builder(
    endpoint: &config::Endpoint,
    ca: Option<&std::path::Path>,
) -> Result<reqwest::ClientBuilder> {
    let builder = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(2));
    match (endpoint.tls, ca) {
        (true, Some(ca)) => super::tls::trust(builder, ca),
        (true, None) => {
            anyhow::bail!("An https:// worker control URL needs a certificate authority")
        }
        (false, _) => Ok(builder),
    }
}
impl Client {
    pub(crate) async fn events(&self, path: &str) -> Result<reqwest::Response> {
        let response = tokio::time::timeout(
            Duration::from_secs(2),
            self.task(self.stream.get(format!("{}{path}", self.url)))
                .bearer_auth(&self.token)
                .send(),
        )
        .await
        .map_err(|_| Unavailable::default())?
        .map_err(Unavailable::from_send)?;
        if response.status() == reqwest::StatusCode::SERVICE_UNAVAILABLE {
            return Err(Unavailable::default().into());
        }
        ensure!(
            response.status().is_success(),
            "Controller denied executor stream"
        );
        Ok(response)
    }
    pub fn new(control: &crate::config::WorkerControl, token: &str) -> Result<Self> {
        super::model::token(token)?;
        let endpoint = config::url(&control.url)?;
        let ca = control.ca_file.as_deref();
        Ok(Self {
            http: builder(&endpoint, ca)?
                .timeout(Duration::from_secs(2))
                .build()?,
            stream: builder(&endpoint, ca)?.build()?,
            url: endpoint.origin,
            token: token.into(),
            task: None,
        })
    }
    pub fn for_task(mut self, task: Option<String>) -> Self {
        self.task = task;
        self
    }
    fn task(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.task {
            Some(task) => request.header("x-ayati-cloud-task", task),
            None => request,
        }
    }
    pub fn with_token(&self, token: &str) -> Result<Self> {
        super::model::token(token)?;
        // Transport has no default Authorization header or cookie store. Each
        // request supplies its own capability; cloning only shares connections.
        Ok(Self {
            http: self.http.clone(),
            stream: self.stream.clone(),
            url: self.url.clone(),
            token: token.into(),
            task: self.task.clone(),
        })
    }
    pub async fn post<T: DeserializeOwned>(&self, path: &str, body: &impl Serialize) -> Result<T> {
        self.post_timeout(path, body, 2).await
    }
    pub async fn post_timeout<T: DeserializeOwned>(
        &self,
        path: &str,
        body: &impl Serialize,
        seconds: u64,
    ) -> Result<T> {
        let mut response = self
            .task(self.http.post(format!("{}{path}", self.url)))
            .bearer_auth(&self.token)
            .json(body)
            .timeout(Duration::from_secs(seconds))
            .send()
            .await
            .map_err(Unavailable::from_send)?;
        if [429, 502, 503, 504].contains(&response.status().as_u16()) {
            return Err(Unavailable::default().into());
        }
        if response.status() == reqwest::StatusCode::CONFLICT {
            return Err(Conflict.into());
        }
        ensure!(
            response.status().is_success(),
            "Controller denied machine request (HTTP {})",
            response.status().as_u16()
        );
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| Unavailable::default())? {
            ensure!(
                bytes.len() + chunk.len() <= 524288,
                "Controller response exceeds protocol limit"
            );
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes)
            .map_err(|_| anyhow::anyhow!("Invalid controller protocol response"))
    }
    pub async fn retry<T: DeserializeOwned>(
        &self,
        path: &str,
        body: &impl Serialize,
        seconds: u64,
    ) -> Result<T> {
        self.retry_waiting(path, body, seconds, seconds).await
    }
    /// Like `retry`, but keeps waiting up to `unreachable_seconds` while no
    /// controller answers at all (a deployment starting controllers and
    /// executors together).
    pub async fn retry_waiting<T: DeserializeOwned>(
        &self,
        path: &str,
        body: &impl Serialize,
        seconds: u64,
        unreachable_seconds: u64,
    ) -> Result<T> {
        let start = tokio::time::Instant::now();
        loop {
            match self.post(path, body).await {
                Err(error) if error.is::<Unavailable>() => {
                    let unreachable = error
                        .downcast_ref::<Unavailable>()
                        .is_some_and(|e| e.unreachable);
                    let limit = if unreachable {
                        unreachable_seconds
                    } else {
                        seconds
                    };
                    if start.elapsed() >= Duration::from_secs(limit) {
                        return Err(error);
                    }
                    let pause = if unreachable { 1000 } else { 150 };
                    tokio::time::sleep(Duration::from_millis(pause)).await;
                }
                result => return result,
            }
        }
    }
}

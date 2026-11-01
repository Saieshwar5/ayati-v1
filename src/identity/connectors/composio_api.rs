//! A bounded client for Composio's platform API (v3.1). Provider error bodies,
//! headers and account state are never logged or returned: they can carry
//! tokens. Writes are never retried here.
use anyhow::{Context, Result, ensure};
use futures_util::StreamExt;
use reqwest::{Client, Method, Url};
use serde_json::Value;
use std::time::Duration;

pub(super) const OFFICIAL: &str = "https://backend.composio.dev/api/v3.1/";
/// Larger results are refused rather than cut, so nothing half-read is used.
const MAX_RESPONSE: usize = 256 * 1024;

#[derive(Clone)]
pub(super) struct ComposioApi {
    client: Client,
    base: Url,
    /// Read on every request, so an operator can rotate the key without a restart.
    key_file: std::path::PathBuf,
}

/// How a request ended.
pub(super) enum Reply {
    Ok(Value),
    /// Composio answered with this HTTP status.
    Status(u16),
}

impl ComposioApi {
    /// The official endpoint, or an explicit loopback fixture for tests.
    pub(super) fn new(base: &str, key_file: std::path::PathBuf) -> Result<Self> {
        let base = Url::parse(base).context("Invalid Composio endpoint")?;
        ensure!(
            base.as_str() == OFFICIAL
                || (base.scheme() == "http"
                    && base.host_str() == Some("127.0.0.1")
                    && base.path().ends_with('/')
                    && base.query().is_none()),
            "Composio endpoint must be the official HTTPS API or an explicit loopback fixture"
        );
        let client = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(45))
            .build()?;
        let api = Self {
            client,
            base,
            key_file,
        };
        api.key()?;
        Ok(api)
    }

    /// Whether this client talks to a local test fixture.
    pub(super) fn is_fixture(&self) -> bool {
        self.base.scheme() == "http"
    }

    fn key(&self) -> Result<String> {
        let key = crate::identity::private_config::read(&self.key_file)?
            .trim()
            .to_owned();
        ensure!(
            !key.is_empty() && key.len() <= 512 && key.bytes().all(|b| b.is_ascii_graphic()),
            "Invalid Composio API key file"
        );
        ensure!(
            !key.starts_with("uak_"),
            "Use a Composio project API key, not a personal user key"
        );
        Ok(key)
    }

    pub(super) async fn send(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<Reply> {
        let mut request = self
            .client
            .request(method, self.base.join(path)?)
            .header("x-api-key", self.key()?);
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = request
            .send()
            .await
            .map_err(|_| anyhow::anyhow!("Composio did not answer"))?;
        let status = response.status();
        if !status.is_success() {
            return Ok(Reply::Status(status.as_u16()));
        }
        let mut stream = response.bytes_stream();
        let mut bytes = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| anyhow::anyhow!("Composio's answer was interrupted"))?;
            ensure!(
                bytes.len() + chunk.len() <= MAX_RESPONSE,
                "Composio's answer was too large"
            );
            bytes.extend_from_slice(&chunk);
        }
        if bytes.is_empty() {
            return Ok(Reply::Ok(Value::Null));
        }
        Ok(Reply::Ok(
            serde_json::from_slice(&bytes).context("Composio returned an invalid answer")?,
        ))
    }

    /// A request that must succeed.
    pub(super) async fn request(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<Value> {
        match self.send(method, path, body).await? {
            Reply::Ok(value) => Ok(value),
            Reply::Status(code) => anyhow::bail!("Composio refused the request (HTTP {code})"),
        }
    }
}

/// Composio ids and slugs used in paths: plain, bounded text only.
pub(super) fn id(value: &str) -> Result<&str> {
    ensure!(
        !value.is_empty()
            && value.len() <= 128
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b)),
        "Invalid Composio identifier"
    );
    Ok(value)
}

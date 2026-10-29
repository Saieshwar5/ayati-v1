//! Bounded machine transport. Never follows redirects, uses proxies or prints credentials/bodies.
use super::{config, model::*};
use anyhow::{Result, ensure};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::time::Duration;

#[derive(Debug)]
pub(crate) struct Unavailable;
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
    url: String,
    token: String,
}
impl Client {
    pub(crate) async fn events(&self, path: &str) -> Result<reqwest::Response> {
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(2))
            .build()?;
        let response = tokio::time::timeout(
            Duration::from_secs(2),
            client
                .get(format!("{}{path}", self.url))
                .bearer_auth(&self.token)
                .send(),
        )
        .await
        .map_err(|_| Unavailable)?
        .map_err(|_| Unavailable)?;
        if response.status() == reqwest::StatusCode::SERVICE_UNAVAILABLE {
            return Err(Unavailable.into());
        }
        ensure!(
            response.status().is_success(),
            "Controller denied executor stream"
        );
        Ok(response)
    }
    pub fn new(url: &str, token: &str) -> Result<Self> {
        super::model::token(token)?;
        Ok(Self {
            http: reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(2))
                .build()?,
            url: config::url(url)?,
            token: token.into(),
        })
    }
    pub fn with_token(&self, token: &str) -> Result<Self> {
        super::model::token(token)?;
        // Transport has no default Authorization header or cookie store. Each
        // request supplies its own capability; cloning only shares connections.
        Ok(Self {
            http: self.http.clone(),
            url: self.url.clone(),
            token: token.into(),
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
            .http
            .post(format!("{}{path}", self.url))
            .bearer_auth(&self.token)
            .json(body)
            .timeout(Duration::from_secs(seconds))
            .send()
            .await
            .map_err(|_| Unavailable)?;
        if [429, 502, 503, 504].contains(&response.status().as_u16()) {
            return Err(Unavailable.into());
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
        while let Some(chunk) = response.chunk().await.map_err(|_| Unavailable)? {
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
        let deadline = tokio::time::Instant::now() + Duration::from_secs(seconds);
        loop {
            match self.post(path, body).await {
                Err(error)
                    if error.is::<Unavailable>() && tokio::time::Instant::now() < deadline =>
                {
                    tokio::time::sleep(Duration::from_millis(150)).await;
                }
                result => return result,
            }
        }
    }
    pub async fn release(
        &self,
        executor: &Registration,
        scope: &Scope,
        reason: &str,
    ) -> Result<()> {
        let _: Value = self
            .retry(
                &format!(
                    "/v1/executors/{}/{}/release",
                    executor.executor, executor.incarnation
                ),
                &Release {
                    scope: scope.clone(),
                    reason: reason.into(),
                },
                3,
            )
            .await?;
        Ok(())
    }
}

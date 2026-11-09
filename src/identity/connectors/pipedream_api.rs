//! A bounded client for Pipedream Connect's REST API (v1). It exchanges the
//! OAuth client credentials for an access token, keeps that token in memory
//! until shortly before it expires and never logs it. Provider error bodies,
//! headers and account answers are never logged or returned: they can carry
//! tokens. Writes are never retried here.
use crate::config::pipedream::{environment_name, project};
use anyhow::{Context, Result, anyhow, ensure};
use futures_util::StreamExt;
use reqwest::{Client, Method, Url};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub(super) const OFFICIAL: &str = "https://api.pipedream.com/v1/";
/// Larger answers are refused rather than cut, so nothing half-read is used.
const MAX_RESPONSE: usize = 512 * 1024;
/// An access token is replaced this many seconds before Pipedream says it expires.
const TOKEN_MARGIN: i64 = 300;

/// Where Cuckoo reaches Pipedream Connect, and with which credentials.
#[derive(Clone, PartialEq, Debug)]
pub(crate) struct Settings {
    pub base: String,
    /// Owned 0600 files holding the OAuth client ID and secret, read on use.
    pub client_id_file: PathBuf,
    pub client_secret_file: PathBuf,
    pub project_id: String,
    /// `development` or `production`.
    pub environment: String,
}
impl Settings {
    /// From the legacy `AYATI_PIPEDREAM_*` settings.
    pub(crate) fn from_config(connectors: &crate::config::Connectors) -> Option<Self> {
        let settings = connectors.pipedream.as_ref()?;
        Some(Self {
            base: settings.base_url.clone().unwrap_or_else(|| OFFICIAL.into()),
            client_id_file: settings.client_id_file.clone(),
            client_secret_file: settings.client_secret_file.clone(),
            project_id: settings.project_id.clone(),
            environment: settings.environment.clone(),
        })
    }

    /// From the providers file's checked `pipedream` entry.
    pub(crate) fn from_file(entry: &super::providers_file::Pipedream, base: &str) -> Self {
        Self {
            base: base.to_owned(),
            client_id_file: entry.client_id_file.clone(),
            client_secret_file: entry.client_secret_file.clone(),
            project_id: entry.project_id.clone(),
            environment: entry.environment.clone(),
        }
    }

    /// The `pipedream` entry of `AYATI_CONNECTOR_PROVIDERS_FILE`, as
    /// designs/connector-layer/contracts.md shows it. The price per call is
    /// read by the provider operations package, not here.
    pub(crate) fn from_entry(entry: &Value) -> Result<Self> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Entry {
            provider: String,
            client_id_file: PathBuf,
            client_secret_file: PathBuf,
            project_id: String,
            environment: String,
            #[serde(default, rename = "usd_per_1k_calls")]
            _usd_per_1k_calls: Option<f64>,
        }
        let entry = Entry::deserialize(entry).context("Invalid Pipedream provider entry")?;
        ensure!(
            entry.provider == "pipedream",
            "Not a Pipedream provider entry"
        );
        let settings = Self {
            base: OFFICIAL.into(),
            client_id_file: entry.client_id_file,
            client_secret_file: entry.client_secret_file,
            project_id: entry.project_id,
            environment: entry.environment,
        };
        settings.check()?;
        Ok(settings)
    }

    fn check(&self) -> Result<()> {
        ensure!(
            self.client_id_file.is_absolute() && self.client_secret_file.is_absolute(),
            "Pipedream's client files must be absolute paths"
        );
        ensure!(
            project(&self.project_id),
            "Invalid Pipedream project ID; it looks like proj_abc123"
        );
        ensure!(
            environment_name(&self.environment),
            "The Pipedream environment must be development or production"
        );
        Ok(())
    }
}

#[derive(Clone)]
pub(super) struct PipedreamApi {
    client: Client,
    base: Url,
    client_id_file: PathBuf,
    client_secret_file: PathBuf,
    project: String,
    environment: String,
    token: Arc<Mutex<Option<Token>>>,
}

/// The cached access token, with a digest of the credentials it was issued for,
/// so a rotated client secret takes effect on the next request.
struct Token {
    credentials: String,
    value: String,
    until: i64,
}

/// How a request ended.
pub(super) enum Reply {
    Ok(Value),
    /// Pipedream answered with this HTTP status (any but 200, 201 and 204).
    Status(u16),
}

impl PipedreamApi {
    /// The official endpoint, or an explicit loopback fixture for tests.
    pub(super) fn new(settings: &Settings) -> Result<Self> {
        settings.check()?;
        let base = Url::parse(&settings.base).context("Invalid Pipedream endpoint")?;
        ensure!(
            base.as_str() == OFFICIAL
                || (base.scheme() == "http"
                    && base.host_str() == Some("127.0.0.1")
                    && base.path().ends_with("/v1/")
                    && base.query().is_none()),
            "Pipedream endpoint must be the official HTTPS API or an explicit loopback fixture"
        );
        let client = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(45))
            .build()?;
        let api = Self {
            client,
            base,
            client_id_file: settings.client_id_file.clone(),
            client_secret_file: settings.client_secret_file.clone(),
            project: settings.project_id.clone(),
            environment: settings.environment.clone(),
            token: Arc::default(),
        };
        api.credentials()?;
        Ok(api)
    }

    /// Whether this client talks to a local test fixture.
    pub(super) fn is_fixture(&self) -> bool {
        self.base.scheme() == "http"
    }

    fn credentials(&self) -> Result<(String, String)> {
        let read = |path: &Path, what: &str| -> Result<String> {
            let text = crate::identity::private_config::read(path)?
                .trim()
                .to_owned();
            ensure!(
                !text.is_empty() && text.len() <= 512 && text.bytes().all(|b| b.is_ascii_graphic()),
                "Invalid Pipedream {what} file"
            );
            Ok(text)
        };
        Ok((
            read(&self.client_id_file, "client ID")?,
            read(&self.client_secret_file, "client secret")?,
        ))
    }

    /// A current access token: the cached one, or a new one from the client
    /// credentials (OAuth client credentials grant, valid for an hour).
    async fn token(&self) -> Result<String> {
        let (id, secret) = self.credentials()?;
        let credentials = crate::identity::provider::digest(&format!("{id}\n{secret}"));
        let now = crate::identity::user::now();
        if let Some(token) = self.cached().as_ref()
            && token.credentials == credentials
            && token.until > now
        {
            return Ok(token.value.clone());
        }
        let body = json!({"grant_type":"client_credentials","client_id":id,"client_secret":secret,"scope":"connect:*"});
        let response = self
            .client
            .post(self.base.join("oauth/token")?)
            .json(&body)
            .send()
            .await
            .map_err(|_| anyhow!("Pipedream did not answer"))?;
        let status = response.status().as_u16();
        ensure!(
            status == 200,
            "Pipedream refused the client credentials (HTTP {status})"
        );
        let answer = read_body(response).await?;
        let value = answer["access_token"]
            .as_str()
            .filter(|t| !t.is_empty() && t.len() <= 4096 && t.bytes().all(|b| b.is_ascii_graphic()))
            .context("Pipedream omitted the access token")?
            .to_owned();
        let lifetime = answer["expires_in"]
            .as_i64()
            .context("Pipedream omitted the token lifetime")?;
        *self.cached() = Some(Token {
            credentials,
            value: value.clone(),
            until: now + lifetime - TOKEN_MARGIN,
        });
        Ok(value)
    }

    fn cached(&self) -> std::sync::MutexGuard<'_, Option<Token>> {
        self.token.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// A Connect API request under this project and environment. `path` is
    /// relative to `/v1/connect/{project_id}/`.
    pub(super) async fn send(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, &str)],
        body: Option<&Value>,
    ) -> Result<Reply> {
        let mut url = self
            .base
            .join(&format!("connect/{}/{path}", self.project))?;
        if !query.is_empty() {
            url.query_pairs_mut().extend_pairs(query);
        }
        let mut request = self
            .client
            .request(method, url)
            .bearer_auth(self.token().await?)
            .header("x-pd-environment", &self.environment);
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = request
            .send()
            .await
            .map_err(|_| anyhow!("Pipedream did not answer"))?;
        let status = response.status().as_u16();
        if status == 401 {
            // The token was revoked or expired early; the next request gets a new one.
            *self.cached() = None;
        }
        if ![200, 201, 204].contains(&status) {
            return Ok(Reply::Status(status));
        }
        Ok(Reply::Ok(read_body(response).await?))
    }

    /// A request that must succeed.
    pub(super) async fn request(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, &str)],
        body: Option<&Value>,
    ) -> Result<Value> {
        match self.send(method, path, query, body).await? {
            Reply::Ok(value) => Ok(value),
            Reply::Status(code) => anyhow::bail!("Pipedream refused the request (HTTP {code})"),
        }
    }
}

async fn read_body(response: reqwest::Response) -> Result<Value> {
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| anyhow!("Pipedream's answer was interrupted"))?;
        ensure!(
            bytes.len() + chunk.len() <= MAX_RESPONSE,
            "Pipedream's answer was too large"
        );
        bytes.extend_from_slice(&chunk);
    }
    if bytes.is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_slice(&bytes).context("Pipedream returned an invalid answer")
}

/// Pipedream ids, keys and slugs used in paths: plain, bounded text only.
pub(super) fn id(value: &str) -> Result<&str> {
    ensure!(
        !value.is_empty()
            && value.len() <= 128
            && !value.starts_with('.')
            && !value.contains("..")
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b)),
        "Invalid Pipedream identifier"
    );
    Ok(value)
}

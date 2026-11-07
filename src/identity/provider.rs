use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use reqwest::{Client, Method};
use serde_json::{Value, json};
use std::time::Duration;

/// WorkOS answered with an error status. `code` is WorkOS's short error code (for
/// example `invalid_request`), kept only when it is a plain identifier.
#[derive(Debug)]
pub struct ProviderError {
    pub status: u16,
    pub code: Option<String>,
}
impl ProviderError {
    /// A 4xx answer is a definite refusal: WorkOS processed the request and created
    /// nothing. 5xx and transport failures leave the outcome unknown.
    pub fn refused(error: &anyhow::Error) -> Option<&Self> {
        error
            .downcast_ref::<Self>()
            .filter(|error| (400..500).contains(&error.status))
    }
}
impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Authentication service returned HTTP {}", self.status)?;
        if let Some(code) = &self.code {
            write!(f, " ({code})")?;
        }
        Ok(())
    }
}
impl std::error::Error for ProviderError {}

pub struct Provider {
    client: Client,
    key: String,
    pub client_id: String,
    base_url: String,
    /// Sessions WorkOS recently confirmed (`provider/recent.rs`).
    trusted: recent::Trusted,
}

impl Provider {
    pub(super) fn simulation() -> Self {
        Self {
            client: Client::new(),
            key: String::new(),
            client_id: "simulation".into(),
            base_url: "simulation://disabled".into(),
            trusted: Default::default(),
        }
    }
    #[cfg(test)]
    pub(super) fn test_provider() -> Self {
        Self {
            client: Client::new(),
            key: String::new(),
            client_id: "client_test".into(),
            base_url: "https://api.workos.com".into(),
            trusted: Default::default(),
        }
    }
    #[cfg(test)]
    pub(super) fn fixture(base_url: String) -> Self {
        let mut provider = Self::test_provider();
        provider.base_url = base_url;
        provider.client = Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        provider
    }
    /// WorkOS client for a validated client ID and API key.
    pub fn new(client_id: String, key: String) -> Result<Self> {
        Ok(Self {
            client: Client::builder()
                .timeout(Duration::from_secs(20))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            key,
            client_id,
            base_url: "https://api.workos.com".into(),
            trusted: Default::default(),
        })
    }

    pub async fn request(&self, method: Method, path: &str, body: Option<Value>) -> Result<Value> {
        let mut request = self
            .client
            .request(method, format!("{}{path}", self.base_url))
            .bearer_auth(&self.key);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request
            .send()
            .await
            .context("Authentication service unavailable")?;
        if !response.status().is_success() {
            let status = response.status().as_u16();
            // Provider bodies can contain tokens and personal data. Never forward them
            // as errors; keep only WorkOS's short machine error code, if it is one.
            let code = response
                .bytes()
                .await
                .ok()
                .filter(|body| body.len() <= 16 * 1024)
                .and_then(|body| serde_json::from_slice::<Value>(&body).ok())
                .and_then(|body| body["code"].as_str().map(str::to_owned))
                .filter(|code| {
                    !code.is_empty()
                        && code.len() <= 64
                        && code
                            .bytes()
                            .all(|b| b.is_ascii_lowercase() || b == b'_' || b.is_ascii_digit())
                });
            return Err(ProviderError { status, code }.into());
        }
        if response.status() == reqwest::StatusCode::NO_CONTENT {
            return Ok(json!({}));
        }
        response
            .json()
            .await
            .context("Invalid authentication service response")
    }

    pub async fn exchange(
        &self,
        code: &str,
        verifier: &str,
        invitation: Option<&str>,
    ) -> Result<Value> {
        let mut body = json!({"client_id":self.client_id, "client_secret":self.key,
            "grant_type":"authorization_code", "code":code, "code_verifier":verifier});
        if let Some(token) = invitation {
            body["invitation_token"] = json!(token);
        }
        self.request(Method::POST, "/user_management/authenticate", Some(body))
            .await
    }

    /// Both checks are required, but neither depends on the other. This result is
    /// never cached here; requests use [`Provider::identity_valid_recently`], which
    /// trusts a positive answer for a bounded time (`provider/recent.rs`).
    pub async fn identity_valid(&self, user: &str, session: &str, email: &str) -> Result<bool> {
        let profile_path = format!("/user_management/users/{user}");
        let (active, current) = tokio::try_join!(
            self.session_active(user, session),
            self.request(Method::GET, &profile_path, None),
        )?;
        Ok(active
            && current["email_verified"] == true
            && current["email"]
                .as_str()
                .is_some_and(|value| value.to_lowercase() == email))
    }

    pub async fn session_active(&self, user: &str, session: &str) -> Result<bool> {
        let mut after = String::new();
        for _ in 0..20 {
            let mut url = reqwest::Url::parse(&format!(
                "https://api.workos.com/user_management/users/{user}/sessions"
            ))?;
            url.query_pairs_mut().append_pair("limit", "100");
            if !after.is_empty() {
                url.query_pairs_mut().append_pair("after", &after);
            }
            let path = format!("{}?{}", url.path(), url.query().unwrap_or_default());
            let result = self.request(Method::GET, &path, None).await?;
            let data = result["data"]
                .as_array()
                .context("Invalid session response")?;
            if data
                .iter()
                .any(|item| item["id"] == session && item["status"] == "active")
            {
                return Ok(true);
            }
            let next = result["list_metadata"]["after"]
                .as_str()
                .unwrap_or_default();
            if data.is_empty() || next.is_empty() || next == after {
                return Ok(false);
            }
            after = next.into();
        }
        Ok(false)
    }
}

pub fn random_token() -> String {
    use ring::rand::{SecureRandom, SystemRandom};
    let mut bytes = [0u8; 32];
    SystemRandom::new()
        .fill(&mut bytes)
        .expect("OS randomness unavailable");
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn digest(value: &str) -> String {
    URL_SAFE_NO_PAD.encode(ring::digest::digest(&ring::digest::SHA256, value.as_bytes()).as_ref())
}

// Only called on tokens obtained directly from a successful server-side WorkOS exchange.
// Browser-supplied JWTs are never accepted or decoded as identity.
pub fn exchanged_session(value: &Value) -> Result<String> {
    let jwt = value["access_token"]
        .as_str()
        .context("Missing session token")?;
    let part = jwt.split('.').nth(1).context("Invalid session token")?;
    let claims: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(part)?)?;
    let id = claims["sid"].as_str().context("Missing provider session")?;
    ensure!(
        id.starts_with("session_") && id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_'),
        "Invalid session ID"
    );
    Ok(id.into())
}

#[cfg(test)]
#[path = "provider_tests.rs"]
mod tests;

#[cfg(test)]
mod invitation_tests;
mod recent;

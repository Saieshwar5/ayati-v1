use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use reqwest::{Client, Method};
use serde_json::{Value, json};
use std::time::Duration;

pub struct Provider {
    client: Client,
    key: String,
    pub client_id: String,
}

impl Provider {
    #[cfg(test)]
    pub(super) fn test_provider() -> Self {
        Self {
            client: Client::new(),
            key: String::new(),
            client_id: "client_test".into(),
        }
    }
    pub fn from_env() -> Result<Self> {
        let key = std::env::var("WORKOS_API_KEY")
            .or_else(|_| std::env::var("WORK_OS_API_KEY"))
            .context("Set WORKOS_API_KEY or WORK_OS_API_KEY in .env")?;
        let client_id = std::env::var("WORKOS_CLIENT_ID")
            .or_else(|_| std::env::var("WORK_OS_CLIENT_ID"))
            .context("Set WORKOS_CLIENT_ID in .env")?;
        ensure!(
            !key.is_empty() && client_id.starts_with("client_"),
            "Invalid WorkOS configuration"
        );
        Ok(Self {
            client: Client::builder()
                .timeout(Duration::from_secs(20))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            key,
            client_id,
        })
    }

    pub async fn request(&self, method: Method, path: &str, body: Option<Value>) -> Result<Value> {
        let mut request = self
            .client
            .request(method, format!("https://api.workos.com{path}"))
            .bearer_auth(&self.key);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request
            .send()
            .await
            .context("Authentication service unavailable")?;
        // Provider bodies can contain tokens and personal data. Never forward them as errors.
        ensure!(
            response.status().is_success(),
            "Authentication service returned HTTP {}",
            response.status().as_u16()
        );
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

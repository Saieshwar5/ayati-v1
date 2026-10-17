use anyhow::{Context, Result, ensure};
use futures_util::StreamExt;
use reqwest::{Client, Method, Url};
use serde_json::{Value, json};
use std::time::Duration;

pub struct Composio {
    client: Client,
    base: Url,
    key: String,
    pub(super) mcp: Option<super::composio_mcp::ConsumerMcp>,
}
impl Composio {
    pub fn configuration_error(&self) -> Option<&'static str> {
        if self.key.starts_with("uak_") {
            Some("Use a Composio project API key for Ayati, rather than a user API key.")
        } else {
            None
        }
    }
    pub fn from_env() -> Result<Option<Self>> {
        let Some(key) = std::env::var("COMPOSIO_API_KEY")
            .ok()
            .filter(|k| !k.trim().is_empty())
        else {
            return Ok(None);
        };
        let base = std::env::var("AYATI_COMPOSIO_BASE_URL")
            .unwrap_or_else(|_| "https://backend.composio.dev/api/v3.1/".into());
        Ok(Some(Self::new(&base, key)?))
    }
    /// An explicit loopback endpoint permits integration tests without real credentials.
    pub fn new(base: &str, key: String) -> Result<Self> {
        let base = Url::parse(base)?;
        ensure!(
            (base.scheme() == "https" && base.host_str() == Some("backend.composio.dev"))
                || (base.scheme() == "http" && base.host_str() == Some("127.0.0.1")),
            "Composio endpoint must be official HTTPS or an explicit loopback fixture"
        );
        ensure!(
            base.username().is_empty()
                && base.password().is_none()
                && base.query().is_none()
                && base.fragment().is_none()
                && base.path().ends_with('/'),
            "Invalid Composio endpoint"
        );
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(45))
            .build()?;
        let mcp = if key.starts_with("ck_") {
            let url = if base.host_str() == Some("127.0.0.1") {
                base.join("mcp")?
            } else {
                Url::parse("https://connect.composio.dev/mcp")?
            };
            Some(super::composio_mcp::ConsumerMcp::new(url, key.clone())?)
        } else {
            None
        };
        Ok(Self {
            client,
            base,
            key,
            mcp,
        })
    }
    pub async fn request(&self, method: Method, path: &str, body: Option<&Value>) -> Result<Value> {
        ensure!(
            !self.is_consumer(),
            "This operation requires Composio Platform APIs; Connect MCP does not expose event subscription management"
        );
        if let Some(error) = self.configuration_error() {
            anyhow::bail!(error);
        }
        let mut request = self
            .client
            .request(method, self.base.join(path)?)
            .header("x-api-key", &self.key);
        if let Some(body) = body {
            request = request.json(body);
        }
        // Never include provider error bodies or headers in logs/API errors.
        let response = request.send().await.map_err(|_| {
            anyhow::anyhow!("Composio did not acknowledge the request; no automatic retry was made")
        })?;
        ensure!(
            response.status().is_success(),
            "Composio rejected the request (HTTP {}); inspect account health before retrying",
            response.status().as_u16()
        );
        let mut stream = response.bytes_stream();
        let mut bytes = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| {
                anyhow::anyhow!("Composio response was interrupted; outcome may be uncertain")
            })?;
            ensure!(
                bytes.len() + chunk.len() <= 256 * 1024,
                "Composio response exceeded the result limit; outcome may be uncertain"
            );
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes)
            .context("Composio returned an invalid response; outcome may be uncertain")
    }
    pub async fn account(&self, id: &str, user: &str, toolkit: &str) -> Result<String> {
        super::identifier(id)?;
        if self.is_consumer() {
            let data = self.consumer_accounts(toolkit).await?;
            return Ok(data
                .as_array()
                .context("Invalid MCP accounts")?
                .iter()
                .find(|a| a["id"] == id)
                .and_then(|a| a["status"].as_str())
                .unwrap_or("UNAVAILABLE")
                .into());
        }
        let account = self
            .request(Method::GET, &format!("connected_accounts/{id}"), None)
            .await?;
        ensure!(
            account["id"] == id
                && account["user_id"] == user
                && account["toolkit"]["slug"] == toolkit,
            "Connected account belongs to another workspace or toolkit"
        );
        if account["is_disabled"] == true || account["auth_config"]["is_disabled"] == true {
            return Ok("INACTIVE".into());
        }
        Ok(account["status"]
            .as_str()
            .context("Account status missing")?
            .into())
    }
    pub async fn execute(
        &self,
        account: &str,
        user: &str,
        tool: &str,
        version: &str,
        arguments: &Value,
    ) -> Result<Value> {
        if let Some(mcp) = &self.mcp {
            let value = mcp.call("COMPOSIO_MULTI_EXECUTE_TOOL", json!({"tools":[{"tool_slug":tool,"account":account,"arguments":arguments}],"sync_response_to_workbench":false})).await?;
            let results = value["data"]["results"]
                .as_array()
                .context("MCP executor omitted results; outcome may be uncertain")?;
            ensure!(
                results.len() == 1 && results[0]["tool_slug"] == tool,
                "MCP execution identity mismatch; outcome may be uncertain"
            );
            let result = &results[0]["response"];
            ensure!(
                results[0]["error"].is_null()
                    && result["successful"] == true
                    && result["error"].is_null(),
                "MCP execution was not confirmed; outcome may be uncertain"
            );
            return Ok(json!({"successful":true,"data":result["data"],"log_id":value["log_id"]}));
        }
        self.request(Method::POST,&format!("tools/execute/{tool}"),Some(&json!({"connected_account_id":account,"user_id":user,"version":version,"arguments":arguments}))).await
    }
}

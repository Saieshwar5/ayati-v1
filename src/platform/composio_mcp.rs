//! Host-only Composio Connect client. Agents never receive its general executor.
use anyhow::{Context, Result, ensure};
use futures_util::StreamExt;
use reqwest::{Client, Url};
use serde_json::{Value, json};
use tokio::sync::Mutex;

pub struct ConsumerMcp {
    client: Client,
    url: Url,
    key: String,
    session: Mutex<Option<Option<String>>>,
}
impl ConsumerMcp {
    pub fn new(url: Url, key: String) -> Result<Self> {
        ensure!(
            (url.scheme() == "https"
                && url.host_str() == Some("connect.composio.dev")
                && url.path() == "/mcp"
                && url.port().is_none())
                || (url.scheme() == "http" && url.host_str() == Some("127.0.0.1")),
            "Consumer credentials require the official Composio Connect MCP endpoint"
        );
        ensure!(
            url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none(),
            "Invalid MCP endpoint"
        );
        Ok(Self {
            client: Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(std::time::Duration::from_secs(45))
                .build()?,
            url,
            key,
            session: Mutex::new(None),
        })
    }
    async fn rpc(
        &self,
        session: Option<&str>,
        method: &str,
        params: Value,
        id: Option<u64>,
    ) -> Result<(Value, Option<String>)> {
        let mut payload = json!({"jsonrpc":"2.0","method":method,"params":params});
        if let Some(id) = id {
            payload["id"] = json!(id);
        }
        let mut request = self
            .client
            .post(self.url.clone())
            .header("x-consumer-api-key", &self.key)
            .header("Accept", "application/json, text/event-stream")
            .header("MCP-Protocol-Version", "2024-11-05")
            .json(&payload);
        if let Some(session) = session {
            request = request.header("Mcp-Session-Id", session);
        }
        let response = request.send().await.map_err(|_| {
            anyhow::anyhow!(
                "Composio MCP did not acknowledge the request; no automatic retry was made"
            )
        })?;
        ensure!(
            response.status().is_success(),
            "Composio MCP rejected the request (HTTP {}); no automatic retry was made",
            response.status().as_u16()
        );
        let session = response
            .headers()
            .get("mcp-session-id")
            .map(|v| v.to_str().map(str::to_owned))
            .transpose()?;
        if id.is_none() {
            return Ok((Value::Null, session));
        }
        let sse = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.starts_with("text/event-stream"));
        let mut bytes = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| {
                anyhow::anyhow!("Composio MCP response was interrupted; outcome may be uncertain")
            })?;
            ensure!(
                bytes.len() + chunk.len() <= 1024 * 1024,
                "Composio MCP response exceeds the result limit; outcome may be uncertain"
            );
            bytes.extend_from_slice(&chunk);
            if let Some(value) = parse_reply(&bytes, sse, id.unwrap())? {
                return Ok((value, session));
            }
        }
        anyhow::bail!("Composio MCP returned no matching response; outcome may be uncertain")
    }
    pub async fn call(&self, name: &str, arguments: Value) -> Result<Value> {
        // Keep provider session state private and serialize its initialization.
        let mut session = self.session.lock().await;
        if session.is_none() {
            let (init, returned) = self.rpc(None, "initialize", json!({"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"ayati","version":env!("CARGO_PKG_VERSION")}}), Some(1)).await?;
            ensure!(
                init["protocolVersion"] == "2024-11-05"
                    && init["capabilities"]["tools"].is_object(),
                "Unsupported Composio MCP capabilities"
            );
            self.rpc(
                returned.as_deref(),
                "notifications/initialized",
                json!({}),
                None,
            )
            .await?;
            *session = Some(returned);
        }
        let reply = self
            .rpc(
                session.as_ref().and_then(|s| s.as_deref()),
                "tools/call",
                json!({"name":name,"arguments":arguments}),
                Some(2),
            )
            .await;
        if reply.is_err() {
            // Never repeat this operation. A later deliberate request may
            // establish a fresh transport session after expiry or network loss.
            *session = None;
        }
        let (result, _) = reply?;
        ensure!(
            result["isError"] != true,
            "Composio MCP tool failed; inspect account health before retrying"
        );
        let text = result["content"]
            .as_array()
            .and_then(|v| v.iter().find_map(|v| v["text"].as_str()))
            .context("Composio MCP omitted structured tool output")?;
        let value: Value =
            serde_json::from_str(text).context("Invalid Composio MCP tool output")?;
        ensure!(
            value["successful"] == true
                && value["error"].is_null()
                && value["data"]["success"] != false,
            "Composio MCP operation failed; inspect account health before retrying"
        );
        Ok(value)
    }
}

fn parse_reply(bytes: &[u8], sse: bool, id: u64) -> Result<Option<Value>> {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return Ok(None);
    };
    let candidates = if sse {
        let normalized = text.replace("\r\n", "\n");
        normalized
            .split("\n\n")
            .take(normalized.split("\n\n").count().saturating_sub(1))
            .map(|event| {
                event
                    .lines()
                    .filter_map(|l| l.strip_prefix("data:"))
                    .map(|s| s.trim_start())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
    } else {
        vec![text.to_owned()]
    };
    for candidate in candidates {
        let Ok(value) = serde_json::from_str::<Value>(&candidate) else {
            continue;
        };
        if value["id"] != id {
            continue;
        }
        ensure!(
            value["jsonrpc"] == "2.0" && value.get("error").is_none(),
            "Composio MCP protocol request failed"
        );
        return Ok(Some(
            value.get("result").context("Missing MCP result")?.clone(),
        ));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn handles_comments_chunked_sse_and_json_without_exposing_errors() {
        let bytes = b": connected\r\n\r\nevent: message\r\ndata: {\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{\"ok\":true}}\r\n\r\n";
        assert!(
            parse_reply(&bytes[..bytes.len() - 1], true, 2)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            parse_reply(bytes, true, 2).unwrap().unwrap(),
            json!({"ok":true})
        );
        assert!(parse_reply(bytes, true, 3).unwrap().is_none());
        assert!(
            parse_reply(
                br#"{"jsonrpc":"2.0","id":2,"error":{"message":"PRIVATE"}}"#,
                false,
                2
            )
            .unwrap_err()
            .to_string()
            .find("PRIVATE")
            .is_none()
        );
    }
}

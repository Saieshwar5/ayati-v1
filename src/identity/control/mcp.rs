//! Bounded Streamable HTTP client. No redirects, proxy credentials or automatic write retries.
use super::{catalog::Server, model::Operation};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};

pub(crate) struct Remote {
    client: reqwest::Client,
    server: Server,
    session: Option<String>,
    bearer: Option<String>,
}
impl Remote {
    pub async fn connect(server: Server, fingerprint: &str) -> Result<Self> {
        let bearer = server.bearer()?;
        ensure!(
            server.fingerprint_for(&bearer)? == fingerprint,
            "Connection credentials changed"
        );
        let url = super::endpoint(&server.url)?;
        let host = url.host_str().context("Missing MCP host")?;
        let mut builder = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(12));
        if host != "127.0.0.1" {
            let addresses: Vec<_> = tokio::time::timeout(
                std::time::Duration::from_secs(8),
                tokio::net::lookup_host((host, url.port_or_known_default().unwrap())),
            )
            .await
            .context("MCP host resolution timed out")??
            .collect();
            ensure!(
                !addresses.is_empty() && addresses.iter().all(|a| public_ip(a.ip())),
                "MCP endpoint resolved to a restricted address"
            );
            builder = builder.resolve_to_addrs(host, &addresses);
        }
        let mut remote = Self {
            client: builder.build()?,
            server,
            session: None,
            bearer,
        };
        let init = remote.rpc("initialize", json!({"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"cuckoo-control","version":env!("CARGO_PKG_VERSION")}}), Some(1)).await?;
        ensure!(
            init["protocolVersion"] == "2025-11-25" && init["capabilities"]["tools"].is_object(),
            "Unsupported remote MCP protocol or capabilities"
        );
        remote
            .rpc("notifications/initialized", json!({}), None)
            .await?;
        Ok(remote)
    }
    pub async fn verify(&mut self, operations: &[Operation]) -> Result<()> {
        self.inventory(operations).await.map(|_| ())
    }
    pub async fn inventory(&mut self, operations: &[Operation]) -> Result<Vec<String>> {
        let listed = self.rpc("tools/list", json!({}), Some(2)).await?;
        let tools = listed["tools"]
            .as_array()
            .filter(|v| v.len() <= 200)
            .context("Remote tool catalog is too large or unavailable")?;
        ensure!(
            listed.get("nextCursor").is_none_or(Value::is_null),
            "Paginated remote catalogs need additional support"
        );
        for op in operations {
            let matches: Vec<_> = tools
                .iter()
                .filter(|tool| tool["name"] == op.tool)
                .collect();
            ensure!(
                matches.len() == 1 && matches[0]["inputSchema"] == op.schema,
                "Remote operation schema changed; review the connection before use"
            );
        }
        let mut names = std::collections::BTreeSet::new();
        for tool in tools {
            let name = tool["name"]
                .as_str()
                .filter(|s| !s.is_empty() && s.len() <= 100 && !s.chars().any(char::is_control))
                .context("Unsupported remote tool name")?;
            ensure!(names.insert(name.to_owned()), "Duplicate remote tool name");
        }
        Ok(names.into_iter().collect())
    }
    pub async fn call(&mut self, operation: &Operation, arguments: &Value) -> Result<Value> {
        operation.arguments(arguments)?;
        let result = self
            .rpc(
                "tools/call",
                json!({"name":operation.tool,"arguments":arguments}),
                Some(3),
            )
            .await?;
        ensure!(
            result["isError"] != true
                && (result["content"].is_array() || result["structuredContent"].is_object()),
            "Remote operation did not confirm success"
        );
        Ok(result)
    }
    async fn rpc(&mut self, method: &str, params: Value, id: Option<u64>) -> Result<Value> {
        let mut payload = json!({"jsonrpc":"2.0","method":method,"params":params});
        if let Some(id) = id {
            payload["id"] = json!(id);
        }
        let mut request = self
            .client
            .post(&self.server.url)
            .header("Accept", "application/json, text/event-stream")
            .header("MCP-Protocol-Version", "2025-11-25")
            .json(&payload);
        if let Some(token) = &self.bearer {
            request = request.bearer_auth(token);
        }
        if let Some(session) = &self.session {
            request = request.header("Mcp-Session-Id", session);
        }
        let mut response = request
            .send()
            .await
            .map_err(|_| anyhow::anyhow!("Remote MCP did not acknowledge the request"))?;
        ensure!(
            response.status().is_success(),
            "Remote MCP rejected the request"
        );
        if let Some(session) = response.headers().get("mcp-session-id") {
            let session = session.to_str().context("Invalid MCP session")?;
            ensure!(
                session.len() <= 256 && session.bytes().all(|b| b.is_ascii_graphic()),
                "Invalid MCP session"
            );
            self.session = Some(session.into());
        }
        if id.is_none() {
            return Ok(Value::Null);
        }
        let sse = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.starts_with("text/event-stream"));
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| anyhow::anyhow!("Remote MCP result was interrupted"))?
        {
            ensure!(
                bytes.len() + chunk.len() <= 65536,
                "Remote MCP result exceeds 64 KiB"
            );
            bytes.extend_from_slice(&chunk);
            if let Some(value) = super::mcp_reply::parse_reply(&bytes, sse, id.unwrap())? {
                return Ok(value);
            }
        }
        anyhow::bail!("Remote MCP returned no matching result")
    }
}
fn public_ip(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(ip) => {
            !(ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.is_broadcast()
                || ip.is_multicast()
                || ip.octets()[0] == 0
                || ip.octets()[0] >= 240
                || (ip.octets()[0] == 100 && (64..=127).contains(&ip.octets()[1])))
        }
        // IPv6 private, mapped IPv4 and special-purpose ranges need separate proof.
        std::net::IpAddr::V6(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn remote_dns_cannot_reach_metadata_private_loopback_or_mapped_addresses() {
        for address in [
            "127.0.0.1",
            "169.254.169.254",
            "10.0.0.1",
            "192.168.1.1",
            "172.16.0.1",
            "100.64.0.1",
            "0.0.0.0",
            "224.0.0.1",
            "::1",
            "::ffff:169.254.169.254",
        ] {
            assert!(!public_ip(address.parse().unwrap()), "{address}");
        }
        assert!(public_ip("8.8.8.8".parse().unwrap()));
        for url in [
            "http://169.254.169.254/mcp",
            "https://user:password@example.test/mcp",
            "https://example.test/mcp?token=value",
            "https://example.test/mcp#private",
        ] {
            assert!(crate::identity::control::endpoint(url).is_err());
        }
    }
}

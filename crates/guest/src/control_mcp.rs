//! Trusted forwarding tools; workspace authority and provider credentials stay in the controller.
use anyhow::{Result, ensure};
use rmcp::{ErrorData, RoleServer, ServerHandler, ServiceExt, model::*, service::RequestContext};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
pub const PREFIX: &str = "ayati_control__";
pub fn observed(name: &str, input: &Value) -> bool {
    match name.strip_prefix(PREFIX) {
        Some("operations") => input == &json!({}),
        Some("request_action") => input.as_object().is_some_and(|v| {
            v.len() == 3
                && v.get("connection").is_some_and(Value::is_string)
                && v.get("operation").is_some_and(Value::is_string)
                && v.get("arguments").is_some_and(Value::is_object)
        }),
        _ => false,
    }
}
struct ControlTools;
impl ServerHandler for ControlTools {
    fn get_info(&self) -> ServerConfig {
        let mut info = ServerConfig::new(ServerCapabilities::builder().enable_tools().build());
        info.instructions=Some("Discover assigned operations, then request exact actions through Cuckoo. The controller enforces Allow/Ask first/Block. A pending action needs human approval in Cuckoo; report its ID and wait without repeating it. An uncertain action must be reconciled before new writes. Never claim pending or denied work succeeded. The same exact operation and fields within this task reuse the saved receipt, including after recovery.".into());
        info
    }
    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> std::result::Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult{tools:vec![
            json!({"name":"operations","description":"List this task's currently assigned business operations, input fields and permission modes.","inputSchema":{"type":"object","properties":{},"required":[],"additionalProperties":false}}),
            json!({"name":"request_action","description":"Request one supported operation with exact fields. Cuckoo validates current task authority, records it, executes allowed work or creates an exact approval request. Inspect the returned state. Repeating identical fields in the same task returns the original receipt and never retries an uncertain write.","inputSchema":{"type":"object","properties":{"connection":{"type":"string"},"operation":{"type":"string"},"arguments":{"type":"object"}},"required":["connection","operation","arguments"],"additionalProperties":false}})
        ].into_iter().map(|v|serde_json::from_value(v).unwrap()).collect(),..Default::default()})
    }
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResponse, ErrorData> {
        let arguments = json!(request.arguments.unwrap_or_default());
        let exchange = async {
            ensure!(
                observed(&format!("{PREFIX}{}", request.name), &arguments),
                "Unsupported forwarding request"
            );
            let address = std::env::var("AYATI_CONTROL_ADDRESS")?;
            let parsed: std::net::SocketAddr = address.parse()?;
            ensure!(
                parsed.ip() == std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
                "Control forwarding requires loopback"
            );
            let body=json!({"token":std::env::var("AYATI_CONTROL_TOKEN")?,"tool":request.name,"arguments":arguments}).to_string();
            ensure!(body.len() <= 16000, "Control request exceeds limit");
            let mut stream = tokio::net::TcpStream::connect(parsed).await?;
            stream.write_all(format!("POST /control HTTP/1.0\r\nHost: {address}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await?;
            let mut reply = Vec::new();
            (&mut stream).take(262145).read_to_end(&mut reply).await?;
            ensure!(reply.len() <= 262144, "Control result exceeds limit");
            let offset = reply
                .windows(4)
                .position(|v| v == b"\r\n\r\n")
                .ok_or_else(|| anyhow::anyhow!("Invalid control result"))?
                + 4;
            let value: Value = serde_json::from_slice(&reply[offset..])?;
            ensure!(
                value.get("error").is_none() && value.get("result").is_some(),
                "Control operation denied"
            );
            Ok::<_, anyhow::Error>(value["result"].clone())
        };
        let value = match tokio::time::timeout(std::time::Duration::from_secs(48), exchange).await {
            Ok(Ok(value)) => value,
            _ => {
                json!({"state":"unavailable","message":"Operation denied or unavailable. Inspect saved actions before retrying."})
            }
        };
        let mut result = if value["state"].as_str().is_some_and(|s| {
            [
                "denied",
                "rejected",
                "invalidated",
                "uncertain",
                "unavailable",
            ]
            .contains(&s)
        }) {
            CallToolResult::error(vec![ContentBlock::text(value.to_string())])
        } else {
            CallToolResult::success(vec![ContentBlock::text(value.to_string())])
        };
        result.structured_content = Some(value);
        Ok(result.into())
    }
}
pub async fn serve() -> Result<()> {
    ControlTools
        .serve(rmcp::transport::stdio())
        .await?
        .waiting()
        .await?;
    Ok(())
}

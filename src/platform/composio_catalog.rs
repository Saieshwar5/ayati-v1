use super::composio::Composio;
use anyhow::{Context, Result, ensure};
use reqwest::Method;
use serde_json::{Value, json};

impl Composio {
    pub fn is_consumer(&self) -> bool {
        self.mcp.is_some()
    }
    pub async fn consumer_search(&self, query: &str) -> Result<Value> {
        ensure!(
            !query.trim().is_empty() && query.len() <= 200,
            "Describe the operations to discover in 1–200 bytes"
        );
        let reply = self
            .mcp
            .as_ref()
            .context("Consumer MCP is not configured")?
            .call(
                "COMPOSIO_SEARCH_TOOLS",
                json!({"queries":[{"use_case":query}],"session":{"generate_id":true}}),
            )
            .await?;
        ensure!(
            reply["data"]["tool_schemas"].is_object(),
            "Composio MCP omitted discovered schemas"
        );
        Ok(reply["data"].clone())
    }
    pub(super) fn consumer_definition(tool: &str, schema: &Value) -> Result<Value> {
        super::identifier(tool)?;
        ensure!(
            schema["tool_slug"] == tool && schema["input_schema"].is_object(),
            "Composio MCP omitted the operation schema"
        );
        let toolkit = schema["toolkit"]
            .as_str()
            .context("MCP omitted toolkit")?
            .to_ascii_lowercase();
        super::identifier(&toolkit)?;
        let definition = json!({"slug":tool,"toolkit":{"slug":toolkit},"description":schema["description"],"input_parameters":schema["input_schema"]});
        let digest = ring::digest::digest(&ring::digest::SHA256, definition.to_string().as_bytes());
        let version = format!(
            "mcp_{}",
            digest
                .as_ref()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        let mut definition = definition;
        definition["version"] = json!(version);
        Ok(definition)
    }
    pub async fn tool_definition(&self, tool: &str, version: &str) -> Result<Value> {
        if let Some(mcp) = &self.mcp {
            // Caller supplies only a slug previously discovered and saved by the owner.
            let reply = mcp
                .call("COMPOSIO_GET_TOOL_SCHEMAS", json!({"tool_slugs":[tool]}))
                .await?;
            return Self::consumer_definition(tool, &reply["data"]["tool_schemas"][tool]);
        }
        self.request(
            Method::GET,
            &format!("tools/{tool}?version={version}"),
            None,
        )
        .await
    }
    pub async fn search_tools(&self, toolkit: &str, query: &str) -> Result<Value> {
        if self.is_consumer() {
            let data = self
                .consumer_search(&format!(
                    "{toolkit}: {}",
                    if query.trim().is_empty() {
                        "Discover available operations"
                    } else {
                        query
                    }
                ))
                .await?;
            let mut tools = Vec::new();
            for (slug, schema) in data["tool_schemas"].as_object().unwrap() {
                // Search also returns related tool stubs. Hydrate only tools
                // from the requested toolkit, not every suggested plan step.
                if schema["toolkit"]
                    .as_str()
                    .is_none_or(|t| !t.eq_ignore_ascii_case(toolkit))
                {
                    continue;
                }
                let hydrated;
                let schema = if schema["input_schema"].is_object() {
                    schema
                } else {
                    hydrated = self.tool_definition(slug, "discovered").await?;
                    tools.push(hydrated);
                    continue;
                };
                let definition = Self::consumer_definition(slug, schema)?;
                if definition["toolkit"]["slug"] == toolkit {
                    tools.push(definition);
                }
            }
            return Ok(json!({"items":tools}));
        }
        let mut url = reqwest::Url::parse("https://backend.composio.dev/api/v3.1/tools")?;
        url.query_pairs_mut()
            .append_pair("toolkit_slug", toolkit)
            .append_pair("limit", "30")
            .append_pair("include_deprecated", "false")
            .append_pair("query", query);
        self.request(
            Method::GET,
            &format!("tools?{}", url.query().unwrap()),
            None,
        )
        .await
    }
    pub async fn consumer_accounts(&self, toolkit: &str) -> Result<Value> {
        super::identifier(toolkit)?;
        let data = self
            .consumer_search(&format!("{toolkit} account operations"))
            .await?;
        ensure!(
            data["toolkit_connection_statuses"]
                .as_array()
                .is_some_and(|rows| rows.iter().any(|v| v["toolkit"] == toolkit)),
            "Toolkit was not discovered by Composio MCP"
        );
        let value=self.mcp.as_ref().unwrap().call("COMPOSIO_MANAGE_CONNECTIONS",json!({"toolkits":[{"name":toolkit,"action":"list"}],"session_id":data["session"]["id"]})).await?;
        let accounts = value["data"]["results"][toolkit]["accounts"]
            .as_array()
            .context("MCP omitted account list")?;
        // Strip user_info, tokens and other account state at the host boundary.
        Ok(json!(accounts.iter().map(|a|json!({"id":a["id"],"status":a["status"].as_str().unwrap_or("unavailable").to_ascii_uppercase(),"alias":a["alias"]})).collect::<Vec<_>>()))
    }
}

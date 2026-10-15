use rmcp::{ErrorData, RoleServer, ServerHandler, ServiceExt, model::*, service::RequestContext};

pub struct ProofTools;

impl ServerHandler for ProofTools {
    fn get_info(&self) -> ServerConfig {
        let mut info = ServerConfig::new(ServerCapabilities::builder().enable_tools().build());
        info.instructions = Some(
            "Only synthetic local demonstration data; no external accounts or actions.".into(),
        );
        info
    }

    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let schema =
            serde_json::json!({"type":"object","properties":{},"additionalProperties":false})
                .as_object()
                .unwrap()
                .clone();
        Ok(ListToolsResult {
            tools: vec![Tool::new(
                "get_demo_business",
                "Read the synthetic business profile. Requires owner approval in this proof.",
                schema,
            )],
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        if request.name != "get_demo_business"
            || request.arguments.as_ref().is_some_and(|a| !a.is_empty())
        {
            return Ok(CallToolResult::error(vec![ContentBlock::text(
                "Denied: unknown operation or unexpected arguments.",
            )])
            .into());
        }
        Ok(CallToolResult::success(vec![ContentBlock::text("Synthetic business: Demo Shop. Product: sample notebook. Stock: 12. This is test data, not a connected store.")]).into())
    }
}

pub async fn serve() -> anyhow::Result<()> {
    ProofTools
        .serve(rmcp::transport::stdio())
        .await?
        .waiting()
        .await?;
    Ok(())
}

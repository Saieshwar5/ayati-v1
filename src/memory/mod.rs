use crate::storage::{Store, memory::Remember, work::RunClaim};
use anyhow::{Result, ensure};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, State},
    routing::post,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

pub struct MemoryAccess {
    pub store: Arc<Store>,
    default_enabled: bool,
    platform: Option<Arc<crate::platform::Platform>>,
    agent_id: String,
    active: Mutex<Option<(String, RunClaim, tokio_util::sync::CancellationToken)>>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    token: String,
    tool: String,
    arguments: Value,
}
impl MemoryAccess {
    pub fn new(store: Arc<Store>) -> Self {
        Self {
            store,
            default_enabled: true,
            platform: None,
            agent_id: String::new(),
            active: Mutex::new(None),
        }
    }
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.default_enabled = enabled;
        self
    }
    pub fn with_platform(
        mut self,
        platform: Option<Arc<crate::platform::Platform>>,
        agent: &str,
    ) -> Self {
        self.platform = platform;
        self.agent_id = agent.into();
        self
    }
    async fn workspace_call(&self, request: Request) -> Result<Value> {
        let (claim, cancel) = {
            let active = self
                .active
                .lock()
                .map_err(|_| anyhow::anyhow!("Runtime unavailable"))?;
            let (token, claim, cancel) = active
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("No active business run"))?;
            ensure!(
                token == &request.token && !cancel.is_cancelled(),
                "Stale business run"
            );
            (claim.clone(), cancel.clone())
        };
        ensure!(self.store.claim_active(&claim)?, "Inactive business run");
        let platform = self
            .platform
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Business tools are unavailable"))?;
        let name = format!("{}{}", ayati_guest::workspace_mcp::PREFIX, request.tool);
        ensure!(
            ayati_guest::workspace_mcp::observed(&name, &request.arguments),
            "Invalid business operation"
        );
        let call = self
            .store
            .consume_workspace_permission(&claim, &name, &request.arguments)?;
        let result = match request.tool.as_str() {
            "connector_tools" => Ok(json!({"grants":platform.grants(&self.agent_id)?})),
            "panels_list" => Ok(json!({"panels":platform.panels(Some(&self.agent_id))?})),
            "apps_list" => Ok(json!({"apps":platform.apps(Some(&self.agent_id))?})),
            "app_source" => Ok(platform.app_source(
                &self.agent_id,
                request.arguments["app_id"].as_str().unwrap(),
                request.arguments.get("revision").and_then(|v| v.as_i64()),
            )?),
            "app_write" => {
                let input = serde_json::from_value(request.arguments)?;
                platform
                    .write_app(
                        crate::platform::ExecutionRun {
                            agent: &self.agent_id,
                            store: &self.store,
                            claim: &claim,
                            cancel: &cancel,
                        },
                        &input,
                    )
                    .await
            }
            "app_records" => {
                let input: crate::platform::app_records::RecordCommand =
                    serde_json::from_value(request.arguments["command"].clone())?;
                platform
                    .agent_app_records(
                        crate::platform::ExecutionRun {
                            agent: &self.agent_id,
                            store: &self.store,
                            claim: &claim,
                            cancel: &cancel,
                        },
                        request.arguments["app_id"].as_str().unwrap(),
                        &input,
                    )
                    .await
            }
            "panel_write" => {
                let input = serde_json::from_value(request.arguments)?;
                platform
                    .write_panel(
                        crate::platform::ExecutionRun {
                            agent: &self.agent_id,
                            store: &self.store,
                            claim: &claim,
                            cancel: &cancel,
                        },
                        &input,
                    )
                    .await
            }
            "schedule_create" => {
                let input: crate::platform::panel_schedules::PanelSchedule =
                    serde_json::from_value(request.arguments)?;
                if let Some(panel) = &input.panel_id {
                    platform.ensure_panel_owner(&self.agent_id, panel)?;
                }
                let schedule = self
                    .store
                    .add_schedule(&crate::storage::intake::NewSchedule {
                        id: input.id,
                        content: input.content,
                        run_at_ms: input.run_at_ms,
                        interval_seconds: input.interval_seconds,
                    })?;
                if let Some(panel) = &input.panel_id {
                    platform.bind_panel_schedule(&self.agent_id, panel, &schedule.id)?;
                }
                Ok(json!(schedule))
            }
            "connector_execute" => {
                let args = &request.arguments;
                platform
                    .execute_connector(
                        crate::platform::ExecutionRun {
                            agent: &self.agent_id,
                            store: &self.store,
                            claim: &claim,
                            cancel: &cancel,
                        },
                        args["connection_id"].as_str().unwrap(),
                        args["tool"].as_str().unwrap(),
                        &args["arguments"],
                    )
                    .await
            }
            _ => anyhow::bail!("Unknown business operation"),
        };
        let status = if result.as_ref().is_ok_and(|v| v["outcome"] != "uncertain") {
            "completed"
        } else {
            "uncertain"
        };
        self.store
            .observe_tool_status(&claim, &call, status, None)?;
        result
    }
    pub fn arm(&self, token: &str, claim: &RunClaim, cancel: tokio_util::sync::CancellationToken) {
        *self.active.lock().unwrap() = Some((token.into(), claim.clone(), cancel));
    }
    pub fn disarm(&self) {
        *self.active.lock().unwrap() = None;
    }
    fn call(&self, request: Request) -> Result<Value> {
        // Keep this lock through the transaction so disarm fences in-flight calls.
        let active = self.active.lock().unwrap();
        let (token, claim, cancel) = active
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No active memory run"))?;
        ensure!(
            !cancel.is_cancelled() && token == &request.token,
            "Stale or unauthorized memory run"
        );
        ensure!(
            self.store
                .profile_edit()?
                .and_then(|p| p.memory)
                .unwrap_or(self.default_enabled),
            "Memory is disabled"
        );
        let scope = self.store.memory_scope(claim)?;
        match request.tool.as_str() {
            "memory_search" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Input {
                    query: String,
                }
                let input: Input = serde_json::from_value(request.arguments)?;
                Ok(
                    json!({"memories":self.store.search_memory(&scope,&input.query)?,"budget_bytes":8000}),
                )
            }
            "memory_source" => {
                ensure!(
                    request.arguments == json!({}),
                    "Unexpected source arguments"
                );
                self.store.memory_source(claim)
            }
            "memory_remember" => {
                let input: Remember = serde_json::from_value(request.arguments)?;
                let source = self.store.memory_source(claim)?;
                ensure!(
                    source["id"].as_str() == Some(&input.source_id),
                    "Only this run's owner message is a writable source"
                );
                Ok(json!(self.store.remember(&scope, &input)?))
            }
            _ => anyhow::bail!("Unknown memory tool"),
        }
    }
}
pub fn routes(access: Arc<MemoryAccess>) -> Router {
    Router::new()
        .route("/memory", post(call))
        .layer(DefaultBodyLimit::max(16000))
        .with_state(access)
}
async fn call(
    State(access): State<Arc<MemoryAccess>>,
    Json(request): Json<Request>,
) -> Json<Value> {
    let result = if ayati_guest::workspace_mcp::tools()
        .iter()
        .any(|t| t["name"] == request.tool)
    {
        access.workspace_call(request).await
    } else {
        access.call(request)
    };
    Json(match result {
        Ok(value) => json!({"result":value}),
        Err(_) => {
            json!({"error":"Memory request denied: invalid scope, source, stale run, conflicting revision or forgotten evidence. Inspect current memory and source before retrying."})
        }
    })
}
pub struct HostMemoryServer(tokio::task::JoinHandle<()>);
impl Drop for HostMemoryServer {
    fn drop(&mut self) {
        self.0.abort();
    }
}
impl HostMemoryServer {
    pub async fn start(access: Arc<MemoryAccess>) -> Result<(Self, u16)> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        Ok((
            Self(tokio::spawn(async move {
                let _ = axum::serve(listener, routes(access)).await;
            })),
            port,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inactive_and_stale_tokens_cannot_read_or_write_memory() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(&dir.path().join("state.sqlite")).unwrap());
        store.enqueue("Useful owner preference").unwrap();
        let claim = store.claim_next("test").unwrap().unwrap();
        let access = MemoryAccess::new(store.clone());
        let request = |token: &str| Request {
            token: token.into(),
            tool: "memory_source".into(),
            arguments: json!({}),
        };
        assert!(access.call(request("new")).is_err());
        access.arm("new", &claim, tokio_util::sync::CancellationToken::new());
        assert!(access.call(request("old")).is_err());
        assert!(access.call(request("new")).is_ok());
        assert!(
            access
                .call(Request {
                    token: "new".into(),
                    tool: "memory_search".into(),
                    arguments: json!({"query":"","agent_id":"other"})
                })
                .is_err()
        );
        let cancelled = tokio_util::sync::CancellationToken::new();
        access.arm("cancelled", &claim, cancelled.clone());
        cancelled.cancel();
        assert!(access.call(request("cancelled")).is_err());
        access.disarm();
        assert!(access.call(request("new")).is_err());
        access.arm("next", &claim, tokio_util::sync::CancellationToken::new());
        store.finish_work(&claim, "completed", "").unwrap();
        assert!(access.call(request("next")).is_err());
    }
    #[test]
    fn disabled_memory_rejects_even_a_valid_active_run_token() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(&dir.path().join("state.sqlite")).unwrap());
        let input = crate::storage::profile::UpdateProfile {
            name: "Agent".into(),
            role: String::new(),
            model: None,
            memory: Some(false),
            browser: None,
            expected_revision: 0,
        };
        store
            .update_profile(&input, "", "fixture", true, false)
            .unwrap();
        store.enqueue("Owner input").unwrap();
        let claim = store.claim_next("test").unwrap().unwrap();
        let access = MemoryAccess::new(store);
        access.arm("valid", &claim, tokio_util::sync::CancellationToken::new());
        for (tool, arguments) in [
            ("memory_source", json!({})),
            ("memory_search", json!({"query":"report"})),
            ("memory_remember", json!({})),
        ] {
            let error = access
                .call(Request {
                    token: "valid".into(),
                    tool: tool.into(),
                    arguments,
                })
                .unwrap_err();
            assert!(error.to_string().contains("disabled"));
        }
    }
}

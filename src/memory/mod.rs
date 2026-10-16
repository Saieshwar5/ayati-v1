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
            active: Mutex::new(None),
        }
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
        .layer(DefaultBodyLimit::max(12000))
        .with_state(access)
}
async fn call(
    State(access): State<Arc<MemoryAccess>>,
    Json(request): Json<Request>,
) -> Json<Value> {
    Json(match access.call(request) {
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
}

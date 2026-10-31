//! A local Streamable HTTP MCP server and its operator file, for tests.
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

pub(super) const WORKSPACE: &str = "00000000-0000-4000-8000-000000000053";
const BEARER: &str = "fixture-mcp-bearer";

#[derive(Default)]
pub(super) struct Fake {
    pub fail_call: bool,
    pub calls: Vec<(String, Value)>,
}
type Shared = Arc<Mutex<Fake>>;

pub(super) struct FakeMcp {
    pub state: Shared,
    pub file: std::path::PathBuf,
    bearer_file: std::path::PathBuf,
    _dir: tempfile::TempDir,
}

fn order_schema() -> Value {
    json!({"type":"object","properties":{"order":{"type":"string"}},"required":["order"],"additionalProperties":false})
}

impl FakeMcp {
    pub(super) async fn start() -> Self {
        let state: Shared = Arc::default();
        let router = Router::new()
            .route("/mcp", post(rpc))
            .with_state(state.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/mcp", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let dir = tempfile::tempdir().unwrap();
        let bearer_file = dir.path().join("bearer");
        write_private(&bearer_file, BEARER);
        let file = dir.path().join("servers.json");
        let profile = json!([{"id":"orders","workspace":WORKSPACE,"name":"Orders","account":"Shop orders","url":url,
            "bearer_file":bearer_file,"operations":[
                {"id":"order.read","title":"Read an order","tool":"read_order","effect":"read","schema":order_schema()},
                {"id":"order.update","title":"Update an order","tool":"update_order","effect":"write","schema":order_schema()}]}]);
        write_private(&file, &profile.to_string());
        Self {
            state,
            file,
            bearer_file,
            _dir: dir,
        }
    }
    /// The operator rotated the account's credential.
    pub(super) fn rotate(&self) {
        write_private(&self.bearer_file, "rotated-bearer");
    }
    pub(super) fn restore(&self) {
        write_private(&self.bearer_file, BEARER);
    }
}

fn write_private(path: &std::path::Path, text: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, text).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
}

async fn rpc(
    State(s): State<Shared>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> axum::response::Response {
    if headers.get("authorization").and_then(|v| v.to_str().ok())
        != Some(&format!("Bearer {BEARER}"))
    {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let id = body["id"].clone();
    let result = match body["method"].as_str().unwrap_or_default() {
        "initialize" => {
            json!({"protocolVersion":"2025-11-25","capabilities":{"tools":{}},"serverInfo":{"name":"fixture","version":"1"}})
        }
        "notifications/initialized" => return StatusCode::ACCEPTED.into_response(),
        "tools/list" => {
            json!({"tools":[{"name":"read_order","inputSchema":order_schema()},{"name":"update_order","inputSchema":order_schema()}]})
        }
        "tools/call" => {
            let mut state = s.lock().unwrap();
            state.calls.push((
                body["params"]["name"].as_str().unwrap().into(),
                body["params"]["arguments"].clone(),
            ));
            if state.fail_call {
                json!({"isError":true,"content":[{"type":"text","text":"refused"}]})
            } else {
                json!({"content":[{"type":"text","text":"done"}]})
            }
        }
        _ => return StatusCode::BAD_REQUEST.into_response(),
    };
    Json(json!({"jsonrpc":"2.0","id":id,"result":result})).into_response()
}

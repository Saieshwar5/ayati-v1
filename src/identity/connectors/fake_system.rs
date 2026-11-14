//! A company's own MCP server for tests: StudyHub, the tutoring app in the
//! connected systems plan. It answers with an older MCP version (2025-06-18),
//! needs a key, and its tools can change between calls.
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

pub(crate) const KEY: &str = "studyhub-key";

#[derive(Default)]
pub(crate) struct Fake {
    pub calls: Vec<(String, Value)>,
    /// Calls answer with an error result.
    pub fail: bool,
    /// The server refuses every request (its key was replaced).
    pub refuse: bool,
    /// Tools listed differently now: a new input schema, or `None` when gone.
    pub changes: std::collections::HashMap<String, Option<Value>>,
}
pub(crate) type Shared = Arc<Mutex<Fake>>;

pub(crate) struct FakeSystem {
    pub state: Shared,
    pub url: String,
}

/// StudyHub's tools: reads, one change, one Cuckoo can't check, and one that
/// takes the student as `subject`.
pub(crate) fn tools() -> Vec<Value> {
    vec![
        json!({"name":"get_marks","title":"Read exam marks","description":"A student's exam marks",
            "annotations":{"readOnlyHint":true},
            "inputSchema":{"$schema":"http://json-schema.org/draft-07/schema#","type":"object",
                "properties":{"student":{"type":"string","minLength":1}},"required":["student"]}}),
        json!({"name":"list_homework","description":"A student's open homework","annotations":{"readOnlyHint":true},
            "inputSchema":{"type":"object","properties":{"student":{"type":"string"}},"required":["student"]}}),
        json!({"name":"save_study_plan","description":"Saves a study plan","annotations":{"readOnlyHint":false},
            "inputSchema":{"type":"object","properties":{"student":{"type":"string"},"plan":{"type":"string"}},"required":["student","plan"]}}),
        json!({"name":"export_students","description":"Every student",
            "inputSchema":{"type":"object","properties":{"filter":{"type":"object","properties":{"class":{"type":"string"}}}}}}),
        json!({"name":"get_profile","description":"One student's class and weak subjects","annotations":{"readOnlyHint":true},
            "inputSchema":{"type":"object","properties":{"subject":{"type":"string"}},"required":["subject"]}}),
    ]
}

impl FakeSystem {
    pub(crate) async fn start() -> Self {
        let state: Shared = Arc::default();
        let router = Router::new()
            .route("/mcp", post(rpc))
            .with_state(state.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/mcp", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        Self { state, url }
    }
}

/// A sealing key for tests, written once per process.
pub(crate) fn secrets() {
    use std::os::unix::fs::PermissionsExt;
    let path = std::env::temp_dir().join(format!("ayati-test-secrets-{}", std::process::id()));
    if !path.exists() {
        std::fs::write(&path, "ab".repeat(32)).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    super::systems::configure(Some(path));
}

async fn rpc(
    State(s): State<Shared>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> axum::response::Response {
    if headers.get("authorization").and_then(|v| v.to_str().ok()) != Some(&format!("Bearer {KEY}"))
        || s.lock().unwrap().refuse
    {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let id = body["id"].clone();
    let result = match body["method"].as_str().unwrap_or_default() {
        "initialize" => {
            json!({"protocolVersion":"2025-06-18","capabilities":{"tools":{}},"serverInfo":{"name":"studyhub","version":"1"}})
        }
        "notifications/initialized" => return StatusCode::ACCEPTED.into_response(),
        "tools/list" => {
            let changes = s.lock().unwrap().changes.clone();
            let listed: Vec<Value> = tools()
                .into_iter()
                .filter_map(
                    |mut tool| match changes.get(tool["name"].as_str().unwrap()) {
                        None => Some(tool),
                        Some(None) => None,
                        Some(Some(schema)) => {
                            tool["inputSchema"] = schema.clone();
                            Some(tool)
                        }
                    },
                )
                .collect();
            json!({"tools":listed})
        }
        "tools/call" => {
            let mut state = s.lock().unwrap();
            state.calls.push((
                body["params"]["name"].as_str().unwrap().into(),
                body["params"]["arguments"].clone(),
            ));
            if state.fail {
                json!({"isError":true,"content":[{"type":"text","text":"refused"}]})
            } else {
                json!({"content":[{"type":"text","text":"done"}],"structuredContent":{"ok":true}})
            }
        }
        _ => return StatusCode::BAD_REQUEST.into_response(),
    };
    Json(json!({"jsonrpc":"2.0","id":id,"result":result})).into_response()
}

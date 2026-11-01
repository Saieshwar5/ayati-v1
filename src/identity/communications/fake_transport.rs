//! A local Communication MCP server and its operator file, for tests: a
//! stand-in for a developer's bridge to some messaging platform. Shaped like
//! protocol v1; `conformance_tests` and the gateway tests both drive it.
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

pub(crate) const SERVER: &str = "fixture-chat";
pub(crate) const ADDRESS: &str = "bot-nova";
pub(crate) const BEARER: &str = "fixture-transport-bearer";

#[derive(Clone, Copy, Default, PartialEq)]
pub(crate) enum Mode {
    #[default]
    Accept,
    /// The platform says no.
    Refuse,
    /// The platform sends it, then the answer is lost (HTTP 500).
    LoseAnswer,
    /// Nothing is reachable.
    Down,
    /// Speaks another protocol version.
    OtherVersion,
}

#[derive(Default)]
pub(crate) struct Platform {
    pub mode: Mode,
    /// Events, oldest first. The cursor is an index.
    pub events: Vec<Value>,
    pub sent: Vec<Value>,
    pub delivered: HashMap<String, String>,
}
type Shared = Arc<Mutex<Platform>>;

pub(crate) struct FakeTransport {
    pub state: Shared,
    pub file: std::path::PathBuf,
    _dir: tempfile::TempDir,
}

impl FakeTransport {
    pub(crate) async fn start(workspace: &str) -> Self {
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
        let file = dir.path().join("transport-servers.json");
        let profile = json!([{"id":SERVER,"workspace":workspace,"name":"Fixture chat","url":url,"bearer_file":bearer_file}]);
        write_private(&file, &profile.to_string());
        Self {
            state,
            file,
            _dir: dir,
        }
    }
    pub(crate) fn mode(&self, mode: Mode) {
        self.state.lock().unwrap().mode = mode;
    }
    pub(crate) fn push(&self, event: Value) {
        self.state.lock().unwrap().events.push(event);
    }
    /// A direct message from `sender` to the agent's bot.
    pub(crate) fn message(&self, id: &str, sender: &str, text: &str) {
        self.push(json!({"type":"message","id":id,"endpoint":ADDRESS,
            "sender":{"address":sender,"name":"Claimed Name"},
            "conversation":{"id":format!("D-{sender}"),"kind":"direct"},"text":text}));
    }
    pub(crate) fn sent(&self) -> Vec<Value> {
        self.state.lock().unwrap().sent.clone()
    }
}

fn write_private(path: &std::path::Path, text: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, text).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
}

fn structured(value: Value) -> Value {
    json!({"content":[{"type":"text","text":value.to_string()}],"structuredContent":value})
}

fn call(platform: &mut Platform, name: &str, args: &Value) -> Result<Value, StatusCode> {
    Ok(match name {
        "transport_info" => structured(json!({"name":"Fixture chat","version":"1"})),
        "list_endpoints" => structured(json!({"endpoints":[{"address":ADDRESS,"name":"Nova"}]})),
        "fetch_inbound" => {
            let start: usize = args["after"]
                .as_str()
                .and_then(|a| a.parse().ok())
                .unwrap_or(0);
            let limit = args["limit"].as_u64().unwrap_or(20) as usize;
            let events: Vec<Value> = platform
                .events
                .iter()
                .skip(start)
                .take(limit)
                .cloned()
                .collect();
            let next = (!events.is_empty()).then(|| (start + events.len()).to_string());
            structured(
                json!({"events":events,"next":next.or_else(|| args["after"].as_str().map(str::to_owned))}),
            )
        }
        "send_message" => {
            if platform.mode == Mode::Refuse {
                return Ok(
                    json!({"isError":true,"content":[{"type":"text","text":"Blocked by the platform"}]}),
                );
            }
            let key = args["key"].as_str().unwrap_or_default().to_owned();
            // The same key never sends twice.
            if !platform.delivered.contains_key(&key) {
                platform.sent.push(args.clone());
                platform.delivered.insert(key.clone(), "delivered".into());
            }
            if platform.mode == Mode::LoseAnswer {
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
            structured(
                json!({"state":"accepted","provider_id":format!("m-{}",platform.sent.len())}),
            )
        }
        "message_status" => {
            let state = platform
                .delivered
                .get(args["key"].as_str().unwrap_or_default());
            structured(json!({"state":state.map_or("unknown", String::as_str)}))
        }
        _ => return Err(StatusCode::BAD_REQUEST),
    })
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
    let mut platform = s.lock().unwrap();
    if platform.mode == Mode::Down {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let id = body["id"].clone();
    let result = match body["method"].as_str().unwrap_or_default() {
        "initialize" => {
            let version = if platform.mode == Mode::OtherVersion {
                2
            } else {
                1
            };
            json!({"protocolVersion":"2025-11-25","capabilities":{"tools":{},"experimental":{"cuckoo/transport":{"version":version}}},
                "serverInfo":{"name":"fixture-transport","version":"1"}})
        }
        "notifications/initialized" => return StatusCode::ACCEPTED.into_response(),
        "tools/call" => match call(
            &mut platform,
            body["params"]["name"].as_str().unwrap_or_default(),
            &body["params"]["arguments"],
        ) {
            Ok(result) => result,
            Err(status) => return status.into_response(),
        },
        _ => return StatusCode::BAD_REQUEST.into_response(),
    };
    Json(json!({"jsonrpc":"2.0","id":id,"result":result})).into_response()
}

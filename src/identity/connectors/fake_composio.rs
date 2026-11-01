//! A local stand-in for Composio's v3.1 API, shaped like its documented
//! answers, for tests. Account answers include a token on purpose: Cuckoo must
//! never store or return it.
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub(crate) const TOKEN: &str = "FAKE-UPSTREAM-ACCESS-TOKEN";
pub(crate) const KEY: &str = "ak_fixture_project_key";
pub(crate) const VERSION: &str = "20260901_00";

#[derive(Default)]
pub(crate) struct Fake {
    pub accounts: HashMap<String, (String, String, String)>,
    pub auth_configs: Vec<(String, String)>,
    pub changed: bool,
    pub fail_execute: bool,
    pub executed: Vec<(String, Value)>,
    pub removed: Vec<String>,
    pub bad_keys: usize,
}
type Shared = Arc<Mutex<Fake>>;

pub(crate) struct FakeComposio {
    pub base: String,
    pub state: Shared,
    pub key_file: std::path::PathBuf,
    _dir: tempfile::TempDir,
}
impl FakeComposio {
    pub(crate) async fn start() -> Self {
        use std::os::unix::fs::PermissionsExt;
        let state: Shared = Arc::default();
        let router = Router::new()
            .route(
                "/api/v3.1/auth_configs",
                get(auth_configs).post(create_auth_config),
            )
            .route("/api/v3.1/connected_accounts/link", post(link))
            .route(
                "/api/v3.1/connected_accounts/{id}",
                get(account).delete(remove),
            )
            .route("/api/v3.1/connected_accounts/{id}/revoke", post(revoke))
            .route("/api/v3.1/tools/{slug}", get(tool))
            .route("/api/v3.1/tools/execute/{slug}", post(execute))
            .with_state(state.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/api/v3.1/", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let dir = tempfile::tempdir().unwrap();
        let key_file = dir.path().join("composio-key");
        std::fs::write(&key_file, KEY).unwrap();
        std::fs::set_permissions(&key_file, std::fs::Permissions::from_mode(0o600)).unwrap();
        Self {
            base,
            state,
            key_file,
            _dir: dir,
        }
    }
    /// The person finished signing in at the provider.
    pub(crate) fn sign_in(&self, account: &str) {
        self.set_status(account, "ACTIVE");
    }
    pub(crate) fn set_status(&self, account: &str, status: &str) {
        self.state
            .lock()
            .unwrap()
            .accounts
            .get_mut(account)
            .unwrap()
            .2 = status.into();
    }
}

fn keyed(state: &Shared, headers: &HeaderMap) -> Result<(), StatusCode> {
    if headers.get("x-api-key").and_then(|v| v.to_str().ok()) == Some(KEY) {
        return Ok(());
    }
    state.lock().unwrap().bad_keys += 1;
    Err(StatusCode::UNAUTHORIZED)
}

async fn auth_configs(
    State(s): State<Shared>,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    keyed(&s, &headers)?;
    let toolkit = q.get("toolkit_slug").cloned().unwrap_or_default();
    let items: Vec<Value> = s.lock().unwrap().auth_configs.iter().filter(|(_, t)| *t == toolkit)
        .map(|(id, t)| json!({"id":id,"name":"Default","toolkit":{"slug":t},"status":"ENABLED","is_composio_managed":true,"no_of_connections":0})).collect();
    Ok(Json(
        json!({"items":items,"next_cursor":null,"total_pages":1,"current_page":1,"total_items":items.len()}),
    ))
}

async fn create_auth_config(
    State(s): State<Shared>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    keyed(&s, &headers)?;
    let toolkit = body["toolkit"]["slug"].as_str().unwrap().to_owned();
    assert_eq!(body["auth_config"]["type"], "use_composio_managed_auth");
    let id = format!("ac_{}", uuid::Uuid::new_v4().simple());
    s.lock()
        .unwrap()
        .auth_configs
        .push((id.clone(), toolkit.clone()));
    Ok(Json(
        json!({"toolkit":{"slug":toolkit},"auth_config":{"id":id,"auth_scheme":"OAUTH2","is_composio_managed":true}}),
    ))
}

async fn link(
    State(s): State<Shared>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    keyed(&s, &headers)?;
    let config = body["auth_config_id"].as_str().unwrap().to_owned();
    assert!(
        body["callback_url"]
            .as_str()
            .unwrap()
            .contains("/api/identity/connectors/return/")
    );
    let toolkit = s
        .lock()
        .unwrap()
        .auth_configs
        .iter()
        .find(|(id, _)| *id == config)
        .unwrap()
        .1
        .clone();
    let id = format!("ca_{}", uuid::Uuid::new_v4().simple());
    s.lock()
        .unwrap()
        .accounts
        .insert(id.clone(), (toolkit, config, "INITIATED".into()));
    let host = headers.get("host").unwrap().to_str().unwrap();
    Ok(Json(
        json!({"link_token":"lt_1","redirect_url":format!("http://{host}/sign-in/{id}"),"expires_at":"2026-11-01T00:10:00Z","connected_account_id":id}),
    ))
}

async fn account(
    State(s): State<Shared>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    keyed(&s, &headers)?;
    let state = s.lock().unwrap();
    let (toolkit, config, status) = state.accounts.get(&id).ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(
        json!({"id":id,"status":status,"status_reason":null,"toolkit":{"slug":toolkit},
        "auth_config":{"id":config,"is_disabled":false},"is_disabled":false,"alias":null,
        "state":{"authScheme":"OAUTH2","val":{"status":status,"access_token":TOKEN}},"params":{}}),
    ))
}

async fn revoke(
    State(s): State<Shared>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    keyed(&s, &headers)?;
    let mut state = s.lock().unwrap();
    state.accounts.get_mut(&id).ok_or(StatusCode::NOT_FOUND)?.2 = "REVOKED".into();
    Ok(Json(json!({"success":true})))
}

async fn remove(
    State(s): State<Shared>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    keyed(&s, &headers)?;
    let mut state = s.lock().unwrap();
    state.accounts.remove(&id).ok_or(StatusCode::NOT_FOUND)?;
    state.removed.push(id);
    Ok(Json(json!({"success":true})))
}

fn schema(slug: &str, changed: bool) -> Option<Value> {
    let text = |d: &str| json!({"type":"string","description":d});
    let mut schema = match slug {
        "GMAIL_FETCH_EMAILS" => {
            json!({"type":"object","properties":{"query":text("Gmail search"),"max_results":{"type":"integer"},"page_token":{"type":"string"},"label_ids":{"type":"array","items":{"type":"string"}},"user_id":{"type":"string","default":"me"}},"required":[]})
        }
        "GMAIL_SEND_EMAIL" => {
            json!({"type":"object","properties":{"recipient_email":text("Who gets it"),"subject":{"type":"string"},"body":{"type":"string"},"is_html":{"type":"boolean"},"cc":{"type":"array"},"user_id":{"type":"string"}},"required":[]})
        }
        "GMAIL_CREATE_EMAIL_DRAFT" => {
            json!({"type":"object","properties":{"recipient_email":{"type":"string"},"subject":{"type":"string"},"body":{"type":"string"},"is_html":{"type":"boolean"},"thread_id":{"type":"string"}},"required":[]})
        }
        // A required field Cuckoo doesn't offer: this action stays unavailable.
        "GMAIL_REPLY_TO_THREAD" => {
            json!({"type":"object","properties":{"thread_id":{"type":"string"},"message_body":{"type":"string"},"attachment":{"type":"object"}},"required":["thread_id","attachment"]})
        }
        _ => return None,
    };
    if changed {
        schema["properties"]["recipient_email"] = json!({"type":"array"});
        schema["properties"]["query"] = json!({"type":"object"});
    }
    Some(schema)
}

async fn tool(
    State(s): State<Shared>,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    keyed(&s, &headers)?;
    if q.get("version").is_some_and(|v| v != VERSION) {
        return Err(StatusCode::NOT_FOUND);
    }
    let changed = s.lock().unwrap().changed;
    let input = schema(&slug, changed).ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(
        json!({"slug":slug,"name":slug,"description":"","toolkit":{"slug":"gmail","name":"Gmail","logo":""},
        "input_parameters":input,"output_parameters":{},"no_auth":false,"available_versions":[VERSION],"version":VERSION,
        "scopes":[],"scope_requirements":null,"tags":[],"is_deprecated":false}),
    ))
}

async fn execute(
    State(s): State<Shared>,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    keyed(&s, &headers)?;
    let mut state = s.lock().unwrap();
    let status = state
        .accounts
        .get(body["connected_account_id"].as_str().unwrap())
        .map(|a| a.2.clone());
    assert_eq!(
        status.as_deref(),
        Some("ACTIVE"),
        "execute only runs on an active account"
    );
    assert_eq!(body["version"], VERSION, "the reviewed version is pinned");
    state.executed.push((slug, body["arguments"].clone()));
    if state.fail_execute {
        return Ok(Json(
            json!({"data":{},"error":"Upstream refused","successful":false}),
        ));
    }
    Ok(Json(
        json!({"data":{"echo":body["arguments"]},"error":null,"successful":true,"log_id":"log_1"}),
    ))
}

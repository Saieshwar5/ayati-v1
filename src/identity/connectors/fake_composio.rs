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
    /// The mailbox the profile read answers with; `None` makes it fail.
    pub mailbox: Option<String>,
    pub identity_reads: usize,
    /// A newer version of every tool, and whether its fields differ.
    pub latest: Option<(String, bool)>,
    /// Tools Composio no longer has, at any version.
    pub retired: Vec<String>,
    /// Every request Composio received, answered or not.
    pub requests: usize,
    /// Trigger instances: id to (trigger slug, account, config).
    pub triggers: HashMap<String, (String, String, Value)>,
    pub deleted_triggers: Vec<String>,
    pub upserts: usize,
    /// The project's one webhook subscription.
    pub webhook: Option<Value>,
}
/// The mailbox every fake account signs in as, unless a test changes it.
pub(crate) const MAILBOX: &str = "orders@example.test";
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
        state.lock().unwrap().mailbox = Some(MAILBOX.into());
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
            .route("/api/v3.1/trigger_instances/{slug}/upsert", post(upsert))
            .route(
                "/api/v3.1/trigger_instances/manage/{id}",
                axum::routing::delete(delete_trigger),
            )
            .route(
                "/api/v3.1/webhook_subscriptions",
                get(webhooks).post(create_webhook),
            )
            .route(
                "/api/v3.1/webhook_subscriptions/{id}",
                axum::routing::patch(update_webhook),
            )
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
    state.lock().unwrap().requests += 1;
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
            json!({"type":"object","properties":{"recipient_email":text("Who gets it"),"subject":{"type":"string"},"body":{"type":"string"},"is_html":{"type":"boolean"},
                "cc":{"type":"array","items":{"type":"string"},"description":"Copies"},"bcc":{"type":"array","items":{"type":"object"}},"user_id":{"type":"string"}},"required":[]})
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
    let (changed, latest, retired) = {
        let state = s.lock().unwrap();
        (
            state.changed,
            state.latest.clone(),
            state.retired.contains(&slug),
        )
    };
    // Without a version Composio answers with its newest one.
    let asked = q.get("version");
    let (version, changed) = if retired {
        return Err(StatusCode::NOT_FOUND);
    } else if asked.is_some_and(|v| v == VERSION) || (asked.is_none() && latest.is_none()) {
        (VERSION.to_owned(), changed)
    } else if let Some((newer, differs)) = latest
        .clone()
        .filter(|(newer, _)| asked.is_none_or(|v| v == newer))
    {
        (newer, differs)
    } else {
        return Err(StatusCode::NOT_FOUND);
    };
    let input = schema(&slug, changed).ok_or(StatusCode::NOT_FOUND)?;
    let mut versions = vec![VERSION.to_owned()];
    versions.extend(latest.map(|(v, _)| v));
    Ok(Json(
        json!({"slug":slug,"name":slug,"description":"","toolkit":{"slug":"gmail","name":"Gmail","logo":""},
        "input_parameters":input,"output_parameters":{},"no_auth":false,"available_versions":versions,"version":version,
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
    if slug == "GMAIL_GET_PROFILE" {
        // The identity read: Gmail's profile, wrapped as Composio wraps tool data.
        state.identity_reads += 1;
        return Ok(Json(match &state.mailbox {
            Some(mailbox) => {
                json!({"data":{"emailAddress":mailbox,"messagesTotal":12,"threadsTotal":9,"historyId":"77"},"error":null,"successful":true})
            }
            None => json!({"data":{},"error":"Insufficient scopes","successful":false}),
        }));
    }
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

/// The signing secret the fake's project webhook uses.
pub(crate) const WEBHOOK_SECRET: &str = "whsec_fixture_project_secret";

/// Headers Composio sends with a delivery, signed with `secret`.
pub(crate) fn signed(secret: &str, id: &str, timestamp: i64, body: &[u8]) -> HeaderMap {
    use base64::{Engine, engine::general_purpose::STANDARD};
    let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, secret.as_bytes());
    let mut signed = format!("{id}.{timestamp}.").into_bytes();
    signed.extend_from_slice(body);
    let signature = STANDARD.encode(ring::hmac::sign(&key, &signed));
    let mut headers = HeaderMap::new();
    headers.insert("webhook-id", id.parse().unwrap());
    headers.insert("webhook-timestamp", timestamp.to_string().parse().unwrap());
    headers.insert(
        "webhook-signature",
        format!("v1,{signature}").parse().unwrap(),
    );
    headers
}

/// A V3 trigger message for `trigger`, as Composio delivers it.
pub(crate) fn message(
    id: &str,
    workspace: &str,
    account: &str,
    trigger: &str,
    data: Value,
) -> Value {
    json!({"id":id,"type":"composio.trigger.message","metadata":{"log_id":"log_1",
        "trigger_slug":"GMAIL_NEW_GMAIL_MESSAGE","trigger_id":trigger,"connected_account_id":account,
        "auth_config_id":"ac_1","user_id":format!("cuckoo-{workspace}")},"data":data,
        "timestamp":"2026-11-08T10:30:00Z"})
}

/// Same account and trigger: the same instance, as Composio's upsert does.
async fn upsert(
    State(s): State<Shared>,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    keyed(&s, &headers)?;
    let account = body["connected_account_id"].as_str().unwrap().to_owned();
    assert!(body["user_id"].as_str().unwrap().starts_with("cuckoo-"));
    let mut state = s.lock().unwrap();
    state.upserts += 1;
    let existing = state
        .triggers
        .iter()
        .find(|(_, (t, a, _))| *t == slug && *a == account)
        .map(|(id, _)| id.clone());
    let (code, id) = match existing {
        Some(id) => (StatusCode::OK, id),
        None => (
            StatusCode::CREATED,
            format!("ti_{}", uuid::Uuid::new_v4().simple()),
        ),
    };
    state
        .triggers
        .insert(id.clone(), (slug, account, body["trigger_config"].clone()));
    Ok((
        code,
        Json(json!({"trigger_id":id,"type":"poll","fallback_reason":null,"polling_interval":2})),
    ))
}

async fn delete_trigger(
    State(s): State<Shared>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    keyed(&s, &headers)?;
    let mut state = s.lock().unwrap();
    state.triggers.remove(&id).ok_or(StatusCode::NOT_FOUND)?;
    state.deleted_triggers.push(id.clone());
    Ok(Json(json!({"trigger_id":id})))
}

fn webhook(body: &Value, id: &str) -> Value {
    json!({"id":id,"webhook_url":body["webhook_url"],"version":body["version"],
        "enabled_events":body["enabled_events"],"secret":WEBHOOK_SECRET,
        "created_at":"2026-11-08T00:00:00Z","updated_at":"2026-11-08T00:00:00Z"})
}

async fn create_webhook(
    State(s): State<Shared>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    keyed(&s, &headers)?;
    let mut state = s.lock().unwrap();
    if state.webhook.is_some() {
        return Err(StatusCode::CONFLICT);
    }
    let saved = webhook(&body, "whs_1");
    state.webhook = Some(saved.clone());
    Ok((StatusCode::CREATED, Json(saved)))
}

async fn webhooks(State(s): State<Shared>, headers: HeaderMap) -> Result<Json<Value>, StatusCode> {
    keyed(&s, &headers)?;
    let items: Vec<Value> = s.lock().unwrap().webhook.iter().cloned().collect();
    Ok(Json(
        json!({"items":items,"next_cursor":null,"total_pages":1,"current_page":1,"total_items":items.len()}),
    ))
}

async fn update_webhook(
    State(s): State<Shared>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    keyed(&s, &headers)?;
    let mut state = s.lock().unwrap();
    if state.webhook.as_ref().map(|w| w["id"].clone()) != Some(json!(id)) {
        return Err(StatusCode::NOT_FOUND);
    }
    let saved = webhook(&body, &id);
    state.webhook = Some(saved.clone());
    Ok(Json(saved))
}

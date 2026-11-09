//! A local stand-in for Pipedream Connect's REST API, shaped like its documented
//! answers, for tests. Account answers include an upstream token on purpose:
//! Cuckoo must never store or return it.
use super::pipedream::Settings;
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// The same leaked upstream token the provider suite looks for.
pub(crate) use super::fake_composio::TOKEN;
pub(crate) const PROJECT: &str = "proj_fixture1";
pub(crate) const VERSION: &str = "0.3.4";
pub(crate) const SIGNING_KEY: &str = "pd-fixture-trigger-signing-key";
pub(crate) const MAILBOX: &str = "Owner@Example.test";
const CLIENT_ID: &str = "oa_fixture_client";
/// The secret the client files start with, and the one an operator rotates to.
const SECRETS: [&str; 2] = ["pd-fixture-secret", "pd-fixture-secret-rotated"];
const ENVIRONMENT: &str = "development";

#[derive(Clone)]
pub(crate) struct FakeAccount {
    pub id: String,
    pub user: String,
    pub app: String,
    pub healthy: bool,
    pub created: String,
    pub updated: String,
}

#[derive(Default)]
pub(crate) struct Fake {
    pub accounts: Vec<FakeAccount>,
    /// Access tokens issued, in order; only the last is accepted.
    pub issued: Vec<String>,
    /// Bodies sent to create Connect tokens.
    pub connect_tokens: Vec<Value>,
    /// List every user's accounts, as if Pipedream ignored the filter.
    pub ignore_user_filter: bool,
    pub changed: bool,
    /// A reply that replaces a successful run: (status, body).
    pub run_reply: Option<(u16, Value)>,
    pub executed: Vec<(String, Value)>,
    pub removed: Vec<String>,
    pub deployed: Vec<Value>,
    pub triggers: Vec<String>,
    pub no_signing_key: bool,
    pub bad_auth: usize,
    changes: usize,
}
type Shared = Arc<Mutex<Fake>>;

pub(crate) struct FakePipedream {
    pub base: String,
    pub state: Shared,
    pub client_id_file: std::path::PathBuf,
    pub client_secret_file: std::path::PathBuf,
    _dir: tempfile::TempDir,
}
impl FakePipedream {
    pub(crate) async fn start() -> Self {
        let state: Shared = Arc::default();
        let connect = "/v1/connect/{project}";
        let router = Router::new()
            .route("/v1/oauth/token", post(token))
            .route(&format!("{connect}/tokens"), post(connect_token))
            .route(&format!("{connect}/accounts"), get(accounts))
            .route(&format!("{connect}/accounts/{{id}}"), delete(remove))
            .route(&format!("{connect}/actions/{{key}}"), get(component))
            .route(&format!("{connect}/triggers/{{key}}"), get(component))
            .route(&format!("{connect}/actions/run"), post(run))
            .route(&format!("{connect}/triggers/deploy"), post(deploy))
            .route(
                &format!("{connect}/deployed-triggers/{{id}}"),
                delete(remove_trigger),
            )
            .with_state(state.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/v1/", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let dir = tempfile::tempdir().unwrap();
        let client_id_file = dir.path().join("pd-client-id");
        let client_secret_file = dir.path().join("pd-client-secret");
        write_private(&client_id_file, CLIENT_ID);
        write_private(&client_secret_file, SECRETS[0]);
        Self {
            base,
            state,
            client_id_file,
            client_secret_file,
            _dir: dir,
        }
    }
    pub(crate) fn settings(&self) -> Settings {
        Settings {
            base: self.base.clone(),
            client_id_file: self.client_id_file.clone(),
            client_secret_file: self.client_secret_file.clone(),
            project_id: PROJECT.into(),
            environment: ENVIRONMENT.into(),
        }
    }
    /// The operator rotated the client secret.
    pub(crate) fn rotate_secret(&self) {
        write_private(&self.client_secret_file, SECRETS[1]);
    }
    /// The person finished Pipedream's sign-in page for the sign-in `binding`.
    pub(crate) fn sign_in(&self, binding: &Value) {
        let mut state = self.state.lock().unwrap();
        let id = format!("apn_{}", state.accounts.len() + 1);
        let time = stamp(&mut state);
        state.accounts.push(FakeAccount {
            id,
            user: binding["user"].as_str().unwrap().into(),
            app: binding["app"].as_str().unwrap().into(),
            healthy: true,
            created: time.clone(),
            updated: time,
        });
    }
    /// The person signed in again to the same upstream account, which Pipedream
    /// updated in place for the sign-in `binding`.
    pub(crate) fn sign_in_again(&self, binding: &Value) {
        let mut state = self.state.lock().unwrap();
        let time = stamp(&mut state);
        let account = state
            .accounts
            .iter_mut()
            .find(|a| a.user == binding["user"] && a.app == binding["app"])
            .unwrap();
        account.updated = time;
    }
    pub(crate) fn set_healthy(&self, healthy: bool) {
        for account in &mut self.state.lock().unwrap().accounts {
            account.healthy = healthy;
        }
    }
}

fn write_private(path: &std::path::Path, text: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, text).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
}

/// A distinct `updated_at` for every change the fake makes.
fn stamp(state: &mut Fake) -> String {
    state.changes += 1;
    format!(
        "2026-11-08T10:{:02}:{:02}.000Z",
        state.changes / 60,
        state.changes % 60
    )
}

fn authorized(state: &Shared, headers: &HeaderMap, project: &str) -> Result<(), StatusCode> {
    let mut state = state.lock().unwrap();
    let bearer = headers.get("authorization").and_then(|v| v.to_str().ok());
    let current = state.issued.last().map(|t| format!("Bearer {t}"));
    if project == PROJECT
        && bearer.is_some()
        && bearer == current.as_deref()
        && headers
            .get("x-pd-environment")
            .and_then(|v| v.to_str().ok())
            == Some(ENVIRONMENT)
    {
        return Ok(());
    }
    state.bad_auth += 1;
    Err(StatusCode::UNAUTHORIZED)
}

async fn token(
    State(s): State<Shared>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    if body["grant_type"] != "client_credentials"
        || body["client_id"] != CLIENT_ID
        || !SECRETS.iter().any(|s| body["client_secret"] == *s)
    {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let mut state = s.lock().unwrap();
    let token = format!("pd-fixture-access-{}", state.issued.len() + 1);
    state.issued.push(token.clone());
    Ok(Json(
        json!({"access_token":token,"token_type":"Bearer","expires_in":3600}),
    ))
}

async fn connect_token(
    State(s): State<Shared>,
    headers: HeaderMap,
    Path(project): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    authorized(&s, &headers, &project)?;
    assert!(
        body["success_redirect_uri"]
            .as_str()
            .unwrap()
            .contains("/api/identity/connectors/return/")
    );
    s.lock().unwrap().connect_tokens.push(body);
    let host = headers.get("host").unwrap().to_str().unwrap();
    let token = format!("ctok_{}", uuid::Uuid::new_v4().simple());
    Ok(Json(
        json!({"token":token,"expires_at":"2026-11-08T10:15:00.000Z",
        "connect_link_url":format!("http://{host}/_static/connect.html?token={token}&connectLink=true")}),
    ))
}

fn account(a: &FakeAccount) -> Value {
    json!({"id":a.id,"name":MAILBOX,"external_id":a.user,"healthy":a.healthy,
        "dead":if a.healthy { Value::Null } else { json!(true) },
        "app":{"id":"oa_gmail","name_slug":a.app,"name":"Gmail","auth_type":"oauth"},
        "created_at":a.created,"updated_at":a.updated,
        "credentials":{"oauth_access_token":TOKEN}})
}

async fn accounts(
    State(s): State<Shared>,
    headers: HeaderMap,
    Path(project): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    authorized(&s, &headers, &project)?;
    let state = s.lock().unwrap();
    let data: Vec<Value> = state
        .accounts
        .iter()
        .filter(|a| state.ignore_user_filter || q.get("external_user_id") == Some(&a.user))
        .filter(|a| q.get("app").is_none_or(|app| *app == a.app))
        .map(account)
        .collect();
    Ok(Json(
        json!({"data":data,"page_info":{"count":data.len(),"total_count":data.len()}}),
    ))
}

async fn remove(
    State(s): State<Shared>,
    headers: HeaderMap,
    Path((project, id)): Path<(String, String)>,
) -> Result<StatusCode, StatusCode> {
    authorized(&s, &headers, &project)?;
    let mut state = s.lock().unwrap();
    let before = state.accounts.len();
    state.accounts.retain(|a| a.id != id);
    if state.accounts.len() == before {
        return Err(StatusCode::NOT_FOUND);
    }
    state.removed.push(id);
    Ok(StatusCode::NO_CONTENT)
}

fn props(key: &str, changed: bool) -> Option<Vec<Value>> {
    let app = json!({"name":"gmail","type":"app","app":"gmail"});
    let list = |name: &str, d: &str| json!({"name":name,"type":"string[]","description":d,"optional":true});
    let body = if changed {
        json!({"name":"body","type":"object"})
    } else {
        json!({"name":"body","type":"string","label":"Email Body"})
    };
    let mail = || {
        vec![
            app.clone(),
            list("to", "Recipient email addresses."),
            list("cc", "Cc recipient email addresses."),
            json!({"name":"subject","type":"string","optional":true}),
            body.clone(),
            json!({"name":"bodyType","type":"string","optional":true,"default":"plaintext","options":["plaintext","html"]}),
            json!({"name":"attachments","type":"string[]","format":"file-ref","optional":true}),
            json!({"name":"syncDir","type":"dir","accessMode":"read","sync":true,"optional":true}),
        ]
    };
    Some(match key {
        "gmail-find-email" => vec![
            app.clone(),
            json!({"name":"q","type":if changed { "object" } else { "string" },"description":"Gmail search query","optional":true}),
            list("labelIds", "Labels"),
            json!({"name":"maxResults","type":"integer","optional":true,"default":25}),
        ],
        "gmail-send-email" | "gmail-create-draft" => mail(),
        "gmail-new-email-received" => vec![
            app.clone(),
            json!({"name":"db","type":"$.service.db"}),
            json!({"name":"timer","type":"$.interface.timer","static":{"intervalSeconds":60}}),
            json!({"name":"withTextPayload","type":"boolean","optional":true}),
            list("labels", "Labels to watch"),
        ],
        _ => return None,
    })
}

async fn component(
    State(s): State<Shared>,
    headers: HeaderMap,
    Path((project, key)): Path<(String, String)>,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    authorized(&s, &headers, &project)?;
    if q.get("version").is_some_and(|v| v != VERSION) {
        return Err(StatusCode::NOT_FOUND);
    }
    let props = props(&key, s.lock().unwrap().changed).ok_or(StatusCode::NOT_FOUND)?;
    let read_only = key == "gmail-find-email";
    Ok(Json(
        json!({"data":{"key":key,"name":key,"version":VERSION,"configurable_props":props,
        "description":"","component_type":"action","annotations":{"readOnlyHint":read_only}}}),
    ))
}

/// The account a run or deployment names must be the external user's own.
fn owned(state: &Fake, body: &Value) -> bool {
    let account = body["configured_props"]["gmail"]["authProvisionId"].as_str();
    state
        .accounts
        .iter()
        .any(|a| Some(a.id.as_str()) == account && a.user == body["external_user_id"] && a.healthy)
}

async fn run(
    State(s): State<Shared>,
    headers: HeaderMap,
    Path(project): Path<String>,
    Json(body): Json<Value>,
) -> Result<Response, StatusCode> {
    authorized(&s, &headers, &project)?;
    let mut state = s.lock().unwrap();
    assert!(
        owned(&state, &body),
        "runs only use the user's healthy account"
    );
    assert_eq!(body["version"], VERSION, "the reviewed version is pinned");
    state
        .executed
        .push((body["id"].as_str().unwrap().into(), body.clone()));
    if let Some((status, reply)) = state.run_reply.clone() {
        return Ok((StatusCode::from_u16(status).unwrap(), Json(reply)).into_response());
    }
    Ok(Json(json!({"exports":{"$summary":"Done"},"os":[],
        "ret":{"echo":body["configured_props"]}}))
    .into_response())
}

async fn deploy(
    State(s): State<Shared>,
    headers: HeaderMap,
    Path(project): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    authorized(&s, &headers, &project)?;
    let mut state = s.lock().unwrap();
    assert!(
        owned(&state, &body),
        "triggers only use the user's healthy account"
    );
    state.deployed.push(body.clone());
    let id = format!("dc_{}", state.deployed.len());
    state.triggers.push(id.clone());
    let key = (!state.no_signing_key).then_some(SIGNING_KEY);
    Ok(Json(
        json!({"data":{"id":id,"type":"DeployedComponent","owner_id":"exu_1","component_id":"sc_1",
        "component_key":body["id"],"configurable_props":[],"configured_props":body["configured_props"],
        "active":true,"created_at":0,"updated_at":0,"name":"fixture","name_slug":"fixture",
        "emit_on_deploy":body["emit_on_deploy"],"webhook_signing_key":key}}),
    ))
}

async fn remove_trigger(
    State(s): State<Shared>,
    headers: HeaderMap,
    Path((project, id)): Path<(String, String)>,
    Query(q): Query<HashMap<String, String>>,
) -> Result<StatusCode, StatusCode> {
    authorized(&s, &headers, &project)?;
    assert!(
        q.get("external_user_id")
            .is_some_and(|u| u.starts_with("cuckoo-"))
    );
    let mut state = s.lock().unwrap();
    let before = state.triggers.len();
    state.triggers.retain(|t| *t != id);
    if state.triggers.len() == before {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
}

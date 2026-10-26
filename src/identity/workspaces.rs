//! Workspace-bound adapter for the existing local client and runtime.
use super::*;
use axum::{Json, extract::Path as RoutePath, http::Method};
use serde_json::json;
use std::path::PathBuf;
use tower::ServiceExt;

pub(super) struct Runtimes {
    root: PathBuf,
    pub chat_wake: tokio::sync::Notify,
    pub rehearsal_wake: tokio::sync::Notify,
    pub rehearsal_tasks: tokio::sync::Mutex<tokio::task::JoinSet<()>>,
    pub chat_slots: Arc<tokio::sync::Semaphore>,
    pub access_gate: tokio::sync::RwLock<()>,
    pub bound: Option<(String, String)>,
    pub stopping: tokio_util::sync::CancellationToken,
    states: tokio::sync::Mutex<HashMap<String, crate::server::WebState>>,
}
impl Runtimes {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            bound: None,
            access_gate: tokio::sync::RwLock::new(()),
            chat_wake: tokio::sync::Notify::new(),
            rehearsal_wake: tokio::sync::Notify::new(),
            rehearsal_tasks: tokio::sync::Mutex::new(tokio::task::JoinSet::new()),
            chat_slots: Arc::new(tokio::sync::Semaphore::new(4)),
            stopping: tokio_util::sync::CancellationToken::new(),
            states: tokio::sync::Mutex::new(HashMap::new()),
        }
    }
    pub fn object_root(&self, id: &str) -> PathBuf {
        self.root.join(id).join("objects")
    }
    pub fn chat_root(&self, id: &str) -> PathBuf {
        self.root.join(id).join("shared-chat")
    }
    pub async fn get(&self, id: &str, port: u16) -> Result<crate::server::WebState> {
        anyhow::ensure!(uuid::Uuid::parse_str(id).is_ok(), "Invalid workspace");
        anyhow::ensure!(
            !self.stopping.is_cancelled(),
            "Workspace server is stopping"
        );
        let mut states = self.states.lock().await;
        if let Some(state) = states.get(id) {
            return Ok(state.clone());
        }
        use std::os::unix::fs::PermissionsExt;
        let adopted = self
            .bound
            .as_ref()
            .is_some_and(|(workspace, _)| workspace == id);
        let data = adoption::resolve(&self.root, &self.bound, id);
        std::fs::create_dir_all(&data)?;
        std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o700))?;
        let mut config = crate::config::Config::from_env()?;
        config.data_dir = data.clone();
        config.prototype = false;
        let store = Arc::new(crate::storage::Store::open(&data.join("ayati.sqlite"))?);
        let files = Arc::new(crate::files::Files::new(&data, store.clone())?);
        let mut runtime = crate::runtime::config::GooseConfig::from_env(&data, &config.model);
        // Never inherit the legacy installation's VM, browser or proof-tool authority.
        if !adopted {
            runtime.vm_manifest = Some(data.join("vms/local-agent/vm.json"));
            runtime.browser = false;
            runtime.proof_tool = false;
        }
        runtime.platform = Some(Arc::new(crate::platform::Platform::open(
            &data.join("business"),
            if adopted {
                crate::platform::composio::Composio::from_env()?
            } else {
                None
            },
        )?));
        runtime.validate_browser()?;
        let ready = runtime.configured();
        let agent = crate::agent::Agent::start_goose(
            store,
            files,
            runtime.clone(),
            (config.vm_idle_seconds > 0)
                .then(|| std::time::Duration::from_secs(config.vm_idle_seconds)),
        );
        let workspace = crate::workspace::Workspace::open_with_template(
            &data,
            config.clone(),
            agent.clone(),
            runtime,
        )?;
        let state = crate::server::WebState {
            agent,
            workspace: Some(workspace),
            model_ready: ready,
            model_name: config.model,
            port,
            shutdown: tokio_util::sync::CancellationToken::new(),
        };
        states.insert(id.into(), state.clone());
        Ok(state)
    }
    pub async fn shutdown(&self) -> Result<()> {
        while self
            .rehearsal_tasks
            .lock()
            .await
            .join_next()
            .await
            .is_some()
        {}
        for state in self.states.lock().await.values() {
            if let Some(workspace) = &state.workspace {
                workspace.begin_shutdown();
                workspace.shutdown().await?;
            }
        }
        Ok(())
    }
}

pub(super) async fn membership(app: &App, user: &User, id: &str) -> ApiResult<String> {
    app.store.role(id, &user.id).await.map_err(|_| {
        Error(
            anyhow::anyhow!("Workspace access denied"),
            StatusCode::FORBIDDEN,
        )
    })
}

// Include the owner's layout-migration hint in the authenticated snapshot so startup
// does not need a second provider-verified request just to read installation context.
pub(super) async fn session_snapshot(app: &App, user: &User) -> Result<serde_json::Value> {
    let mut snapshot = app.store.snapshot(user).await?;
    snapshot["simulation"] = json!(app.simulation.is_some());
    if let Some(entries) = snapshot["workspaces"].as_array_mut() {
        for entry in entries {
            entry["adopted"] = json!(app.runtimes.bound.as_ref().is_some_and(|(id, owner)| {
                entry["id"] == *id && owner == &user.id && entry["role"] == "Owner"
            }));
        }
    }
    Ok(snapshot)
}

pub(super) async fn client() -> Response {
    match tokio::fs::read("web/dist/index.html").await {
        Ok(html) => ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], html).into_response(),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            "Build the client before opening Cuckoo.",
        )
            .into_response(),
    }
}

// New members do not inherit the local-owner authority of the original APIs.
// Explicit resource sharing can be layered onto this boundary later.
pub(super) fn member_view(path: &str, method: &Method) -> Option<serde_json::Value> {
    if method != Method::GET {
        return None;
    }
    match path {
        "/api/home/preferences" => Some(json!({"preferences":null})),
        "/api/home" => Some(json!({"agents":[]})),
        "/api/apps" => Some(json!({"apps":[],"available":true})),
        "/api/panels" => Some(json!({"panels":[],"available":true})),
        "/api/agents" => Some(json!({"agents":[],"creation_available":false,"limit":0})),
        _ => None,
    }
}

pub(super) async fn dispatch(
    State(app): State<Shared>,
    RoutePath((workspace, rest)): RoutePath<(String, String)>,
    request: Request<Body>,
) -> Response {
    let user = match identity(&app, request.headers()).await {
        Ok(user) => user,
        Err(e) => return e.into_response(),
    };
    dispatch_for(app, user, workspace, rest, request).await
}

async fn dispatch_for(
    app: Shared,
    user: User,
    workspace: String,
    rest: String,
    mut request: Request<Body>,
) -> Response {
    let role = match membership(&app, &user, &workspace).await {
        Ok(role) => role,
        Err(e) => return e.into_response(),
    };
    let path = format!("/api/{rest}");
    let adopted = app
        .runtimes
        .bound
        .as_ref()
        .is_some_and(|(id, _)| id == &workspace);
    if adopted
        && role == "Owner"
        && !app
            .runtimes
            .bound
            .as_ref()
            .is_some_and(|(_, owner)| owner == &user.id)
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    if path == "/api/context" {
        return Json(json!({"user":user,"workspace":workspace,"role":role,"resource_access":role == "Owner" && !hosting::hosted(&app),"adopted":adopted && role == "Owner"})).into_response();
    }
    if hosting::hosted(&app) {
        if let Some(mut value) = member_view(&path, request.method()) {
            if matches!(path.as_str(), "/api/apps" | "/api/panels") {
                value["available"] = json!(false);
            }
            return Json(value).into_response();
        }
        return (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error":"This local runtime feature is not available in the hosted workspace yet."}))).into_response();
    }
    if app.simulation.is_some() {
        if path == "/api/agents" {
            let profiles = match collaboration::agents(&app, &user, &workspace).await {
                Ok(p) => p,
                Err(e) => return Error::from(e).into_response(),
            };
            return Json(json!({"agents":profiles.into_iter().map(|p|json!({"profile":{"id":p["id"],"name":p["name"],"role":p["role"],"state":"ready","detail":"Scripted simulation agent","model":"scripted","editable":false},"activity":{"pending_tasks":0,"pending_approvals":0}})).collect::<Vec<_>>(),"creation_available":false,"limit":20})).into_response();
        }
        if let Some(value) = member_view(&path, request.method()) {
            return Json(value).into_response();
        }
        return (StatusCode::FORBIDDEN,Json(json!({"error":"The simulation uses shared conversations; private runtime tools and external services are disabled."}))).into_response();
    }
    if role != "Owner" {
        return resources::member(app, user, workspace, path, request).await;
    }
    // Host-level shutdown and legacy external-connector identity are never tenant APIs.
    if path == "/api/shutdown"
        || (!adopted
            && (path.starts_with("/api/connections") || path.starts_with("/api/event-sources")))
    {
        return (StatusCode::FORBIDDEN, Json(json!({"error":"This service needs workspace-specific configuration before it can be used."}))).into_response();
    }
    let port = app
        .origin
        .rsplit(':')
        .next()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8765);
    let state = match app.runtimes.get(&workspace, port).await {
        Ok(state) => state,
        Err(error) => {
            tracing::error!(%error, "Workspace runtime unavailable");
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"error":"Workspace could not start. Check local setup."})),
            )
                .into_response();
        }
    };
    let uri = format!(
        "{path}{}",
        request
            .uri()
            .query()
            .map(|q| format!("?{q}"))
            .unwrap_or_default()
    );
    *request.uri_mut() = match uri.parse() {
        Ok(uri) => uri,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let session_headers = request.headers().clone();
    // The outer wildcard router has already attached workspace/rest Path values.
    // The business router must extract only its own resource parameters.
    request.extensions_mut().clear();
    let response = crate::server::router(state).oneshot(request).await.unwrap();
    if path == "/api/events" {
        return guarded_stream(app, user, workspace, session_headers, response);
    }
    response
}

fn guarded_stream(
    app: Shared,
    user: User,
    workspace: String,
    headers: axum::http::HeaderMap,
    response: Response,
) -> Response {
    use futures_util::StreamExt;
    let (parts, body) = response.into_parts();
    let token = cookie(&headers, SESSION)
        .map(|token| digest(&token))
        .unwrap_or_default();
    let stream = futures_util::stream::unfold(
        (
            body.into_data_stream(),
            app,
            user,
            workspace,
            headers,
            token,
            tokio::time::Instant::now(),
        ),
        |(mut stream, app, user, workspace, headers, token, mut checked)| async move {
            loop {
                if app.runtimes.stopping.is_cancelled()
                    || membership(&app, &user, &workspace).await.ok().as_deref() != Some("Owner")
                    || !app
                        .store
                        .session(&token)
                        .await
                        .ok()
                        .flatten()
                        .is_some_and(|(active, _, _)| active.id == user.id)
                {
                    return None;
                }
                if checked.elapsed() >= std::time::Duration::from_secs(15) {
                    if identity(&app, &headers).await.is_err() {
                        return None;
                    }
                    checked = tokio::time::Instant::now();
                }
                tokio::select! {
                    chunk = stream.next() => return chunk.map(|c| (c, (stream, app, user, workspace, headers, token, checked))),
                    _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {}
                }
            }
        },
    );
    Response::from_parts(parts, Body::from_stream(stream))
}

#[cfg(test)]
mod tests;

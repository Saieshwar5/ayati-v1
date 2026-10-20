//! Workspace-bound adapter for the existing local client and runtime.
use super::*;
use axum::{Json, extract::Path as RoutePath, http::Method};
use serde_json::json;
use std::path::PathBuf;
use tower::ServiceExt;

pub(super) struct Runtimes {
    root: PathBuf,
    pub stopping: tokio_util::sync::CancellationToken,
    states: tokio::sync::Mutex<HashMap<String, crate::server::WebState>>,
}
impl Runtimes {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            stopping: tokio_util::sync::CancellationToken::new(),
            states: tokio::sync::Mutex::new(HashMap::new()),
        }
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
        let data = self.root.join(id);
        std::fs::create_dir_all(&data)?;
        std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o700))?;
        let mut config = crate::config::Config::from_env()?;
        config.data_dir = data.clone();
        config.prototype = false;
        let store = Arc::new(crate::storage::Store::open(&data.join("ayati.sqlite"))?);
        let files = Arc::new(crate::files::Files::new(&data, store.clone())?);
        let mut runtime = crate::runtime::config::GooseConfig::from_env(&data, &config.model);
        // Never inherit the legacy installation's VM, browser or proof-tool authority.
        runtime.vm_manifest = Some(data.join("vms/local-agent/vm.json"));
        runtime.browser = false;
        runtime.proof_tool = false;
        runtime.platform = Some(Arc::new(crate::platform::Platform::open(
            &data.join("business"),
            None,
        )?));
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
        for state in self.states.lock().await.values() {
            if let Some(workspace) = &state.workspace {
                workspace.begin_shutdown();
                workspace.shutdown().await?;
            }
        }
        Ok(())
    }
}

pub(super) fn membership(app: &App, user: &User, id: &str) -> ApiResult<String> {
    let db = app.store.0.lock().unwrap();
    store::role(&db, id, &user.id).map_err(|_| {
        Error(
            anyhow::anyhow!("Workspace access denied"),
            StatusCode::FORBIDDEN,
        )
    })
}

pub(super) async fn client() -> Response {
    match tokio::fs::read("web/dist/index.html").await {
        Ok(html) => ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], html).into_response(),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            "Build the client before opening Relay.",
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
    let role = match membership(&app, &user, &workspace) {
        Ok(role) => role,
        Err(e) => return e.into_response(),
    };
    let path = format!("/api/{rest}");
    if path == "/api/context" {
        return Json(json!({"user":user,"workspace":workspace,"role":role,"resource_access":role == "Owner"})).into_response();
    }
    if role != "Owner" {
        return match member_view(&path, request.method()) {
            Some(value) => Json(value).into_response(),
            None => (StatusCode::FORBIDDEN, Json(json!({"error":"This resource requires workspace owner access. Membership alone does not grant business-resource access."}))).into_response(),
        };
    }
    // Host-level shutdown and legacy external-connector identity are never tenant APIs.
    if path == "/api/shutdown"
        || path.starts_with("/api/connections")
        || path.starts_with("/api/event-sources")
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
                    || membership(&app, &user, &workspace).ok().as_deref() != Some("Owner")
                    || !app
                        .store
                        .session(&token)
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

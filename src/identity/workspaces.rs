//! Workspace gateway: membership checks, client bootstrap and empty views for
//! workspace APIs that have no shared implementation yet.
use super::*;
use axum::{Json, extract::Path as RoutePath, http::Method};
use serde_json::json;
use std::path::PathBuf;

pub(super) struct Runtimes {
    root: PathBuf,
    pub chat_wake: tokio::sync::Notify,
    pub automation_tasks: tokio::sync::Mutex<tokio::task::JoinSet<()>>,
    pub chat_slots: Arc<tokio::sync::Semaphore>,
    /// Set when a stop is requested: readiness fails while requests still work.
    pub draining: tokio_util::sync::CancellationToken,
    /// Set when draining ends: background loops stop claiming work.
    pub stopping: tokio_util::sync::CancellationToken,
}
impl Runtimes {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            chat_wake: tokio::sync::Notify::new(),
            automation_tasks: tokio::sync::Mutex::new(tokio::task::JoinSet::new()),
            chat_slots: Arc::new(tokio::sync::Semaphore::new(4)),
            draining: tokio_util::sync::CancellationToken::new(),
            stopping: tokio_util::sync::CancellationToken::new(),
        }
    }
    pub fn object_root(&self, id: &str) -> PathBuf {
        self.root.join(id).join("objects")
    }
    pub async fn shutdown(&self) -> Result<()> {
        self.stopping.cancel();
        while self
            .automation_tasks
            .lock()
            .await
            .join_next()
            .await
            .is_some()
        {}
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

pub(super) async fn session_snapshot(app: &App, user: &User) -> Result<serde_json::Value> {
    let mut snapshot = app.store.snapshot(user).await?;
    snapshot["simulation"] = json!(app.simulation.is_some());
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

// Read-only empty views keep older client screens quiet until shared versions exist.
pub(super) fn member_view(path: &str, method: &Method) -> Option<serde_json::Value> {
    if method != Method::GET {
        return None;
    }
    match path {
        "/api/home/preferences" => Some(json!({"preferences":null})),
        "/api/home" => Some(json!({"agents":[]})),
        "/api/apps" => Some(json!({"apps":[],"available":false})),
        "/api/panels" => Some(json!({"panels":[],"available":false})),
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
    request: Request<Body>,
) -> Response {
    let role = match membership(&app, &user, &workspace).await {
        Ok(role) => role,
        Err(e) => return e.into_response(),
    };
    let path = format!("/api/{rest}");
    if path == "/api/context" {
        return Json(json!({"user":user,"workspace":workspace,"role":role})).into_response();
    }
    if app.simulation.is_some() && path == "/api/agents" {
        let profiles = match collaboration::agents(&app, &user, &workspace).await {
            Ok(p) => p,
            Err(e) => return Error::from(e).into_response(),
        };
        return Json(json!({"agents":profiles.into_iter().map(|p|json!({"profile":{"id":p["id"],"name":p["name"],"role":p["role"],"state":"ready","detail":"Scripted simulation agent","model":"scripted","editable":false},"activity":{"pending_tasks":0,"pending_approvals":0}})).collect::<Vec<_>>(),"creation_available":false,"limit":20})).into_response();
    }
    if let Some(value) = member_view(&path, request.method()) {
        return Json(value).into_response();
    }
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({"error":"This workspace feature is not available yet."})),
    )
        .into_response()
}

#[cfg(test)]
mod tests;

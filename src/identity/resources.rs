//! Human resource grants, distinct from agent tool grants and workspace roles.
use super::*;
use anyhow::ensure;
use axum::{Json, extract::Path as RoutePath, http::Method};

use serde::Deserialize;
use serde_json::{Value, json};

pub(super) fn schema(db: &rusqlite::Connection) -> Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS resource_grants(workspace TEXT,user TEXT,kind TEXT,resource TEXT,access TEXT,PRIMARY KEY(workspace,user,kind,resource));
        CREATE TABLE IF NOT EXISTS member_app_sessions(workspace TEXT,user TEXT,app TEXT,hash TEXT,document TEXT,expires INTEGER,PRIMARY KEY(hash));")?;
    Ok(())
}
async fn platform(app: &App, workspace: &str) -> Result<Arc<crate::platform::Platform>> {
    ensure!(
        !hosting::hosted(app),
        "Local business resources are unavailable in hosted workspaces"
    );
    let port = app.origin.rsplit(':').next().unwrap().parse()?;
    app.runtimes
        .get(workspace, port)
        .await?
        .agent
        .platform()
        .context("Business resources unavailable")
}
pub(super) async fn member(
    app: Shared,
    user: User,
    workspace: String,
    path: String,
    request: Request<Body>,
) -> Response {
    let _access = app.runtimes.access_gate.read().await;
    match member_inner(&app, &user, &workspace, &path, request).await {
        Ok(response) => response,
        Err(_) => (
            StatusCode::FORBIDDEN,
            Json(json!({"error":"This resource is unavailable or has not been shared with you."})),
        )
            .into_response(),
    }
}
async fn member_inner(
    app: &Shared,
    user: &User,
    workspace: &str,
    path: &str,
    request: Request<Body>,
) -> Result<Response> {
    let method = request.method().clone();
    if method == Method::GET && ["/api/apps", "/api/panels"].contains(&path) {
        let kind = if path == "/api/apps" { "app" } else { "panel" };
        let permitted = app.store.resource_ids(user, workspace, kind).await?;
        if permitted.is_empty() {
            return Ok(Json(if kind == "app" {
                json!({"apps":[],"available":true})
            } else {
                json!({"panels":[],"available":true})
            })
            .into_response());
        }
        let p = platform(app, workspace).await?;
        if kind == "app" {
            let rows = p
                .published_apps()?
                .into_iter()
                .filter(|v| permitted.iter().any(|id| v["id"] == *id))
                .collect::<Vec<_>>();
            return Ok(Json(json!({"apps":rows,"available":true})).into_response());
        }
        let rows=p.panels(None)?.as_array().context("Invalid panel list")?.iter()
            .filter(|v|permitted.iter().any(|id|v["id"]==*id)&&!v["active"].is_null())
            .map(|v|json!({"id":v["id"],"agent_id":v["agent_id"],"revision":v["active_revision"],"active_revision":v["active_revision"],"active":v["active"],"draft":null,"updates_enabled":false,"state":"active","source_executions":[],"updated_at":v["updated_at"]})).collect::<Vec<_>>();
        return Ok(Json(json!({"panels":rows,"available":true})).into_response());
    }
    let parts: Vec<_> = path.trim_start_matches('/').split('/').collect();
    if parts.len() >= 4 && parts[0] == "api" && parts[1] == "apps" {
        let id = parts[2];
        let permission = app
            .store
            .resource_allowed(user, workspace, "app", id)
            .await?
            .context("No app grant")?;
        let p = platform(app, workspace).await?;
        if parts.len() == 5 && parts[3] == "document" && method == Method::GET {
            let bound = app
                .store
                .app_session_bound(user, workspace, id, parts[4], true)
                .await?;
            ensure!(bound, "Document session belongs to another user");
            let mut response = axum::response::Html(p.app_document(id, parts[4])?).into_response();
            response.headers_mut().insert(
                "content-security-policy",
                crate::platform::app_document::CSP.parse()?,
            );
            return Ok(response);
        }
        ensure!(
            parts.len() == 4 && method == Method::POST,
            "Unsupported app operation"
        );
        let token = request
            .headers()
            .get("x-ayati-app-session")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let bytes = axum::body::to_bytes(request.into_body(), 16000).await?;
        if parts[3] == "session" {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Input {
                revision: i64,
                preview: bool,
            }
            let input: Input = serde_json::from_slice(&bytes)?;
            ensure!(!input.preview, "Only published releases can be shared");
            let session = p.app_session(id, input.revision, false).await?;
            let token = session["session"].as_str().context("Missing app session")?;
            let document = session["document_id"]
                .as_str()
                .context("Missing app document")?;
            app.store
                .app_session_save(user, workspace, id, &provider::digest(token), document)
                .await?;
            return Ok(Json(session).into_response());
        }
        ensure!(
            parts[3] == "records",
            "App management requires owner access"
        );
        let token = token.context("Missing app session")?;
        {
            let bound = app
                .store
                .app_session_bound(user, workspace, id, &provider::digest(&token), false)
                .await?;
            ensure!(bound, "App session belongs to another user");
        }
        let command: crate::platform::app_records::RecordCommand = serde_json::from_slice(&bytes)?;
        ensure!(
            permission == "write"
                || matches!(
                    command,
                    crate::platform::app_records::RecordCommand::List { .. }
                ),
            "Read-only app access"
        );
        return Ok(Json(p.session_records(id, &token, &command).await?).into_response());
    }
    workspaces::member_view(path, &method)
        .map(|v| Json(v).into_response())
        .context("Owner access required")
}

pub(super) fn routes() -> Router<Shared> {
    Router::new().route(
        "/api/identity/workspaces/{workspace}/resource-access",
        get(list).post(grant),
    )
}
async fn list(
    State(app): State<Shared>,
    RoutePath(workspace): RoutePath<String>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    check(
        workspaces::membership(&app, &user, &workspace).await? == "Owner",
        "Only the owner can manage resource access",
    )?;
    hosted_boundary(&app)?;
    let p = platform(&app, &workspace).await?;
    let grants = app.store.resource_grants(&user, &workspace).await?;
    Ok(Json(
        json!({"apps":p.published_apps()?,"panels":p.panels(None)?,"grants":grants}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Grant {
    user: String,
    kind: String,
    resource: String,
    access: Option<String>,
}
async fn grant(
    State(app): State<Shared>,
    RoutePath(workspace): RoutePath<String>,
    headers: axum::http::HeaderMap,
    Json(input): Json<Grant>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let _access = app.runtimes.access_gate.write().await;
    check(
        workspaces::membership(&app, &user, &workspace).await? == "Owner",
        "Only the owner can share resources",
    )?;
    hosted_boundary(&app)?;
    check(
        ["app", "panel"].contains(&input.kind.as_str()),
        "Unknown resource type",
    )?;
    check(
        input
            .access
            .as_deref()
            .is_none_or(|a| a == "read" || (a == "write" && input.kind == "app")),
        "Unknown access type",
    )?;
    let p = platform(&app, &workspace).await?;
    let resources = if input.kind == "app" {
        p.published_apps()?
    } else {
        p.panels(None)?
            .as_array()
            .context("Invalid panels")?
            .clone()
    };
    check(
        resources.iter().any(|r| r["id"] == input.resource),
        "Resource not available",
    )?;
    app.store
        .resource_grant(
            &user,
            &workspace,
            &input.user,
            &input.kind,
            &input.resource,
            input.access.as_deref(),
        )
        .await?;
    Ok(Json(json!({"ok":true})))
}

fn hosted_boundary(app: &App) -> ApiResult<()> {
    if hosting::hosted(app) {
        return Err(Error(
            anyhow::anyhow!("Local business resources are unavailable in hosted workspaces"),
            StatusCode::SERVICE_UNAVAILABLE,
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;

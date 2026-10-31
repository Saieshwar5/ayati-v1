//! Workspace account approval and bounded delegation, separate from agent rules.
use super::*;
use connections_model::{ConnectorRule, SystemApproval, WorkspaceConnection};

pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route(
            "/api/identity/workspaces/{w}/control/connections",
            get(snapshot).post(connect),
        )
        .route(
            "/api/identity/workspaces/{w}/control/systems",
            post(approve),
        )
        .route(
            "/api/identity/workspaces/{w}/control/connector-rules",
            post(connector_rule),
        )
}
async fn snapshot(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let mut value = app.store.workspace_connections(&user, &w).await?;
    // Broken operator configuration must not hide the controls for revoking access.
    value["servers"] = match catalog::servers(&w) {
        Ok(servers) => json!(servers.iter().map(|s| s.public()).collect::<Vec<_>>()),
        Err(_) => {
            value["catalog_error"] = json!(
                "Account configuration is unavailable. Ask your operator to check it. Existing access can still be removed."
            );
            json!([])
        }
    };
    value["execution_available"] = json!(execution_available(&app)?);
    Ok(Json(value))
}
async fn approve(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Json(input): Json<SystemApproval>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    check(
        app.store.role(&w, &user.id).await? == "Admin",
        "Only workspace admins can approve systems",
    )?;
    if input.approved {
        catalog::server(&w, &input.server)?;
    }
    app.store.control_system(&user, &w, &input).await?;
    Ok(Json(json!({"saved":true})))
}
async fn connect(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Json(input): Json<WorkspaceConnection>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    if !input.enabled {
        app.store
            .workspace_connection_save(&user, &w, &input, None)
            .await?;
        return Ok(Json(json!({"saved":true})));
    }
    check(
        execution_available(&app)?,
        "Business tools need controller-managed execution; ask your operator to enable it",
    )?;
    crate::identity::worker_control::model::uuid(&input.id)?;
    app.store.connection_preflight(&user, &w, &input).await?;
    let server = catalog::server(&w, &input.server)?;
    let fingerprint = server.fingerprint()?;
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        let mut remote = mcp::Remote::connect(server.clone(), &fingerprint).await?;
        remote.verify(&server.operations).await
    })
    .await
    .map_err(|_| anyhow::anyhow!("Connection verification timed out"))?
    .map_err(|_| {
        anyhow::anyhow!(
            "The configured account could not be verified. Check its connection and try again."
        )
    })?;
    check(
        catalog::server(&w, &input.server)?.fingerprint()? == fingerprint,
        "Connection changed during verification",
    )?;
    app.store
        .workspace_connection_save(&user, &w, &input, Some((server.public(), &fingerprint)))
        .await?;
    Ok(Json(json!({"saved":true})))
}
async fn connector_rule(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Json(input): Json<ConnectorRule>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    app.store.control_connector_rule(&user, &w, &input).await?;
    Ok(Json(json!({"saved":true})))
}

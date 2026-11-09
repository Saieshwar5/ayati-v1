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
        .route(
            "/api/identity/workspaces/{w}/control/available",
            get(available),
        )
        .route("/api/identity/workspaces/{w}/apps", get(apps))
        .route(
            "/api/identity/workspaces/{w}/apps/systems",
            post(save_system),
        )
        .route(
            "/api/identity/workspaces/{w}/apps/systems/{id}/remove",
            post(remove_system),
        )
        .route(
            "/api/identity/workspaces/{w}/apps/systems/{id}/open",
            get(open_system),
        )
        .route(
            "/api/identity/workspaces/{w}/apps/requests",
            get(requests).post(request),
        )
        .route(
            "/api/identity/workspaces/{w}/apps/requests/{id}",
            post(decide),
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
    value["servers"] = match crate::identity::connectors::apps(&app.store, &w).await {
        Ok(apps) => json!(apps.iter().map(|a| a.public()).collect::<Vec<_>>()),
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
        crate::identity::connectors::provider_for(&app.store, &w, &input.server).await?;
    }
    let released = app.store.control_system(&user, &w, &input).await?;
    let revoked = crate::identity::connectors::routes::release(&w, released).await;
    Ok(Json(json!({"saved":true,"provider_revoked":revoked})))
}
/// Connects an account, or disconnects one. An operator-configured account is
/// verified at once; an app that signs in through its provider returns the
/// sign-in page to open, and finishes when the provider sends the admin back.
async fn connect(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Json(input): Json<WorkspaceConnection>,
) -> ApiResult<Json<Value>> {
    use crate::identity::connectors;
    let user = identity(&app, &headers).await?;
    if !input.enabled {
        let released = app
            .store
            .workspace_connection_bind(&user, &w, &input, None)
            .await?;
        let revoked = connectors::routes::release(&w, released).await;
        return Ok(Json(json!({"saved":true,"provider_revoked":revoked})));
    }
    check(
        execution_available(&app)?,
        "Business tools need controller-managed execution; ask your operator to enable it",
    )?;
    crate::identity::worker_control::model::uuid(&input.id)?;
    app.store.connection_preflight(&user, &w, &input).await?;
    let provider = connectors::provider_for(&app.store, &w, &input.server).await?;
    let started = connectors::connect::start(
        &app.store,
        &app.config.origin,
        &user,
        &w,
        &input,
        provider.as_ref(),
    )
    .await?;
    Ok(Json(match started {
        connectors::connect::Started::Connected => json!({"saved":true}),
        connectors::connect::Started::SignIn(url) => json!({"redirect_url":url}),
    }))
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
async fn available(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let mut value = app.store.control_available(&user, &w).await?;
    value["execution_available"] = json!(app.store.pooled);
    Ok(Json(value))
}
/// The Apps page (AP1): every member reads which services agents use and how.
async fn apps(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.workspace_apps(&user, &w).await?))
}

async fn save_system(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Json(input): Json<crate::identity::postgres::CompanyApp>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.company_app_save(&user, &w, &input).await?))
}
async fn remove_system(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.company_app_remove(&user, &w, &id).await?))
}
/// Opens a company app: Cuckoo checks who may first, then sends the browser on.
async fn open_system(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<axum::response::Redirect> {
    let user = identity(&app, &headers).await?;
    let url = app.store.company_app_open(&user, &w, &id).await?;
    Ok(axum::response::Redirect::to(&url))
}
async fn requests(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.app_requests(&user, &w).await?))
}
async fn request(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Json(input): Json<crate::identity::postgres::AppRequest>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.app_request(&user, &w, &input).await?))
}
async fn decide(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<crate::identity::postgres::AppDecision>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store.app_request_decide(&user, &w, &id, &input).await?,
    ))
}

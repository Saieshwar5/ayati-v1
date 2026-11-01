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
}
async fn snapshot(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let mut value = app.store.workspace_connections(&user, &w).await?;
    // Broken operator configuration must not hide the controls for revoking access.
    value["servers"] = match crate::identity::connectors::apps(&w) {
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
        crate::identity::connectors::provider_for(&w, &input.server)?;
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
    use crate::identity::connectors::{self, Connect};
    use crate::identity::postgres::{
        connector_providers::StartSignIn, workspace_connections::Verified,
    };
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
    let provider = connectors::provider_for(&w, &input.server)?;
    check(
        app.store
            .connector_provider_paused(provider.name())
            .await?
            .is_none(),
        "Cuckoo paused this connector provider; try again later",
    )?;
    let sign_in = uuid::Uuid::new_v4().to_string();
    let callback = format!(
        "{}/api/identity/connectors/return/{sign_in}",
        app.config.origin
    );
    let started = tokio::time::timeout(
        std::time::Duration::from_secs(45),
        provider.connect(&w, &input.server, &callback),
    )
    .await
    .map_err(|_| anyhow::anyhow!("Connecting timed out. Try again."))??;
    match started {
        Connect::Ready(account) => {
            let verified = Verified {
                definition: account.definition,
                fingerprint: account.fingerprint,
                provider: provider.name().into(),
                binding: account.binding,
            };
            let released = app
                .store
                .workspace_connection_bind(&user, &w, &input, Some(&verified))
                .await?;
            connectors::routes::release(&w, released).await;
            Ok(Json(json!({"saved":true})))
        }
        Connect::SignIn {
            binding,
            url,
            expires,
        } => {
            let start = StartSignIn {
                id: &sign_in,
                provider: provider.name(),
                binding: &binding,
                expires,
            };
            if let Err(error) = app
                .store
                .connector_sign_in_start(&user, &w, &input, &start)
                .await
            {
                let _ = provider.disconnect(&w, &input.server, Some(&binding)).await;
                return Err(error.into());
            }
            Ok(Json(json!({"redirect_url":url})))
        }
    }
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

//! Switching an agent's email address on and off. Incoming mail has no route:
//! it arrives on the SES queue (`queue`).
use super::settings;
use crate::identity::{ApiResult, Error, Shared, identity};
use axum::{
    Json, Router,
    extract::{Path as RoutePath, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use serde_json::Value;

pub(crate) fn routes() -> Router<Shared> {
    Router::new()
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{agent}/email",
            get(status),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{agent}/email/activate",
            post(activate),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{agent}/email/deactivate",
            post(deactivate),
        )
}

fn domain() -> ApiResult<String> {
    settings::current()
        .map_err(|e| {
            tracing::warn!(error = %e, "Email configuration is unavailable");
            Error(
                anyhow::anyhow!("Email is unavailable right now. Try again later."),
                StatusCode::SERVICE_UNAVAILABLE,
            )
        })?
        .map(|s| s.domain)
        .ok_or_else(|| {
            Error(
                anyhow::anyhow!("This Cuckoo server doesn't offer email"),
                StatusCode::NOT_FOUND,
            )
        })
}

async fn status(
    State(app): State<Shared>,
    RoutePath((w, agent)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let offered = super::offered();
    Ok(Json(
        app.store.email_status(&user, &w, &agent, offered).await?,
    ))
}
async fn activate(
    State(app): State<Shared>,
    RoutePath((w, agent)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let domain = domain()?;
    Ok(Json(
        app.store.email_activate(&user, &w, &agent, &domain).await?,
    ))
}
async fn deactivate(
    State(app): State<Shared>,
    RoutePath((w, agent)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.email_deactivate(&user, &w, &agent).await?))
}

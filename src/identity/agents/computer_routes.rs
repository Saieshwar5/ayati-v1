//! An agent's computer on its Environment tab: its state and this month's use for anyone
//! who can see the agent, and Reset for its owner or an admin. Turning it on and choosing
//! its size and internet are part of the agent's settings. See runtime/agent-computers.md.
use super::*;

pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{id}/computer",
            get(status),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{id}/computer/reset",
            post(reset),
        )
}

async fn status(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.computer_status(&user, &w, &id).await?))
}

/// Wipes the computer: the next use starts a new one at the chosen size.
async fn reset(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    app.store.computer_reset(&user, &w, &id).await?;
    Ok(Json(app.store.computer_status(&user, &w, &id).await?))
}

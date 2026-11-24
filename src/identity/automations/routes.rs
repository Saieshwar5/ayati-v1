use super::super::*;
use axum::{Json, extract::Path as RoutePath, http::HeaderMap};
use serde_json::Value;
pub(super) fn routes() -> Router<Shared> {
    Router::new().route(
        "/api/identity/workspaces/{w}/agent-profiles/{a}/post-targets",
        get(post_targets),
    )
}
/// Where the agent's owner may post an automation's results.
async fn post_targets(
    State(app): State<Shared>,
    RoutePath((w, a)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.post_targets(&user, &w, &a).await?))
}

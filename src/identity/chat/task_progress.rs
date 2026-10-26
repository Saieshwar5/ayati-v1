use super::*;
pub(super) fn routes() -> Router<Shared> {
    Router::new().route(
        "/api/identity/workspaces/{workspace}/chats/{chat}/messages/{reply}/progress",
        get(progress),
    )
}
async fn progress(
    State(app): State<Shared>,
    RoutePath((w, chat, reply)): RoutePath<(String, String, String)>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        super::super::collaboration::pg(&app)?
            .task_progress(&user, &w, &chat, &reply)
            .await?,
    ))
}

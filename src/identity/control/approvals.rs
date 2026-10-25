use super::*;
pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route(
            "/api/identity/workspaces/{w}/control/approvers",
            post(grant),
        )
        .route("/api/identity/workspaces/{w}/control/approvals", get(inbox))
        .route(
            "/api/identity/workspaces/{w}/control/actions/{id}",
            get(details),
        )
        .route(
            "/api/identity/workspaces/{w}/control/actions/{id}/decision",
            post(decision),
        )
}
async fn grant(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Json(input): Json<approval_model::ApproverGrant>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    pg(&app)?.control_approver_grant(&user, &w, &input).await?;
    Ok(Json(json!({"saved":true})))
}
async fn inbox(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(pg(&app)?.approval_inbox(&user, &w).await?))
}
async fn details(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(pg(&app)?.approval_details(&user, &w, &id).await?))
}
async fn decision(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<model::Decision>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let value = pg(&app)?.approval_details(&user, &w, &id).await?;
    let action = pg(&app)?
        .control_decide(&user, &w, value["agent"].as_str().unwrap(), &id, &input)
        .await?;
    Ok(Json(dispatch(pg(&app)?, &w, action).await?))
}

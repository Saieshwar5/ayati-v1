//! Company rules (the Guard, core-agent CA1): every member reads them; admins change them.
use super::*;
use crate::identity::postgres::control_actions::guard::{ActionMeaning, GuardRule};

pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route(
            "/api/identity/workspaces/{w}/guard/rules",
            get(list).post(save),
        )
        .route(
            "/api/identity/workspaces/{w}/control/action-meanings",
            post(meaning),
        )
        .route(
            "/api/identity/workspaces/{w}/guard/rules/replay",
            post(replay),
        )
        .route(
            "/api/identity/workspaces/{w}/guard/rules/{id}/dismiss",
            post(dismiss),
        )
}
async fn list(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.guard_rules(&user, &w).await?))
}
async fn save(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Json(input): Json<GuardRule>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.guard_rule_save(&user, &w, &input).await?))
}
async fn meaning(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Json(input): Json<ActionMeaning>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store.action_meaning_save(&user, &w, &input).await?,
    ))
}
async fn replay(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Json(input): Json<GuardRule>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.guard_rule_replay(&user, &w, &input).await?))
}
async fn dismiss(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.guard_rule_dismiss(&user, &w, &id).await?))
}

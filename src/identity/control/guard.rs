//! Company rules (the Guard, core-agent CA1): every member reads them; admins change them.
use super::*;
use crate::identity::postgres::control_actions::guard::GuardRule;

pub(super) fn routes() -> Router<Shared> {
    Router::new().route(
        "/api/identity/workspaces/{w}/guard/rules",
        get(list).post(save),
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

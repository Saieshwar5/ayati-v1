//! Lessons an agent learned from its work (schema 121): listed for the people
//! who maintain it, who approve, edit or reject each one and can switch
//! learning off. See runtime/agent-learning.md.
use super::*;
use crate::identity::postgres::agent_learning::Decision;

pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{id}/learning",
            get(list).post(switch),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{id}/learning/{lesson}",
            post(decide),
        )
}

async fn list(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let mut learning = app.store.agent_learning(&user, &w, &id).await?;
    learning["configured"] = json!(app.config.learning_model.is_some());
    Ok(Json(learning))
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Switch {
    enabled: bool,
    revision: i64,
}

async fn switch(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(body): Json<Switch>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let mut learning = app
        .store
        .set_agent_learning(&user, &w, &id, body.enabled, body.revision)
        .await?;
    learning["configured"] = json!(app.config.learning_model.is_some());
    Ok(Json(learning))
}

async fn decide(
    State(app): State<Shared>,
    RoutePath((w, id, lesson)): RoutePath<(String, String, String)>,
    headers: HeaderMap,
    Json(body): Json<Decision>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store
            .decide_lesson(&user, &w, &id, &lesson, &body)
            .await?,
    ))
}

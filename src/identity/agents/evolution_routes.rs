//! Agents that get better on their own (schema 143): an agent's Improving panel
//! and its proposed skills, for its owners and admins, and the workspace's week
//! of learning, for admins. See runtime/evolution.md.
use super::*;
use crate::identity::postgres::evolution::proposals::SkillDecision;

pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{id}/improving",
            get(improving),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{id}/skill-proposals/{proposal}",
            post(decide),
        )
        .route("/api/identity/workspaces/{w}/learning", get(learning))
}

/// Whether this Cuckoo can judge answers and review skills at all.
fn configured(app: &Shared) -> Value {
    json!({"classifier": app.config.classifier_model.is_some() && app.config.model_providers_file.is_some(),
        "reviews": app.config.classifier_model.is_some() && app.config.learning_model.is_some() && app.config.model_providers_file.is_some()})
}

async fn improving(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let mut data = app.store.agent_improving(&user, &w, &id).await?;
    data["configured"] = configured(&app);
    Ok(Json(data))
}

async fn decide(
    State(app): State<Shared>,
    RoutePath((w, id, proposal)): RoutePath<(String, String, String)>,
    headers: HeaderMap,
    Json(input): Json<SkillDecision>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store
            .decide_skill_proposal(&user, &w, &id, &proposal, &input)
            .await?,
    ))
}

async fn learning(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let mut data = app.store.workspace_learning(&user, &w).await?;
    data["configured"] = configured(&app);
    Ok(Json(data))
}

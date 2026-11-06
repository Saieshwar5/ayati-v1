//! Models people choose and the providers that serve them (MG1): a catalog under
//! Cuckoo's own names, and a router that picks a provider for each call. People are
//! never tied to a provider; Cuckoo is never tied to one either.
pub(crate) mod catalog;
pub(crate) mod router;
use super::*;
use axum::{
    Json,
    extract::{Path as RoutePath, Query},
    http::HeaderMap,
};
use serde_json::{Value, json};

pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route("/api/identity/workspaces/{w}/models", get(list))
        .route("/api/identity/workspaces/{w}/usage", get(usage))
        .route("/api/identity/admin/models", get(operations))
        .route(
            "/api/identity/workspaces/{w}/model-budgets",
            get(budgets).post(save_budget),
        )
}

/// The models people may choose, without providers or our costs; empty without a
/// catalog, when agents keep naming a provider and model as before.
pub(super) fn offered(app: &App) -> Result<Value> {
    let Some(files) = &app.store.model_catalog else {
        return Ok(json!({"catalog_on":false,"models":[]}));
    };
    let catalog = files.load()?;
    Ok(
        json!({"catalog_on":true,"models":catalog.models.iter().map(catalog::Model::public).collect::<Vec<_>>()}),
    )
}

/// The workspace's default for new agents: its own choice, else the controller's, else
/// (with a catalog) the first model on offer.
pub(super) fn default_model(app: &App, chosen: Option<String>) -> String {
    let fallback = chosen.unwrap_or_else(|| app.config.default_model.clone());
    match app
        .store
        .model_catalog
        .as_ref()
        .and_then(|files| files.load().ok())
    {
        Some(catalog) if catalog.choosable(&fallback).is_err() => catalog
            .models
            .iter()
            .find(|m| m.status != "retiring")
            .map_or(fallback, |m| m.id.clone()),
        _ => fallback,
    }
}

async fn list(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    workspaces::membership(&app, &user, &w).await?;
    Ok(Json(offered(&app)?))
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct UsageQuery {
    #[serde(default)]
    agent: Option<String>,
    #[serde(default)]
    days: Option<i64>,
}

/// Model use over the last days (MG2): the whole workspace for admins, one agent for
/// its owner, otherwise the use made for the person asking. Catalog models carry
/// their names; our provider costs are never included.
async fn usage(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    Query(input): Query<UsageQuery>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let scope = crate::identity::postgres::model_usage_reports::UsageScope {
        agent: input.agent.as_deref().filter(|a| !a.is_empty()),
        days: input.days.unwrap_or(30),
    };
    let mut report = app.store.model_usage_report(&user, &w, &scope).await?;
    let catalog = app.store.model_catalog.as_ref().and_then(|f| f.load().ok());
    for item in report["by_model"].as_array_mut().into_iter().flatten() {
        let key = item["key"].as_str().unwrap_or_default().to_owned();
        let name = catalog
            .as_ref()
            .and_then(|c| c.find(&key))
            .map_or(key, |m| m.name.clone());
        item["name"] = json!(name);
    }
    Ok(Json(report))
}

/// Monthly model budgets with this month's use (MG3); admins change them.
async fn budgets(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.model_budgets(&user, &w).await?))
}
async fn save_budget(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Json(input): Json<crate::identity::postgres::model_budgets::BudgetInput>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.save_model_budget(&user, &w, &input).await?))
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct OperationsQuery {
    #[serde(default)]
    hours: Option<i64>,
}

/// The operator view (MG4), for Cuckoo's platform administrators only: traffic,
/// failures, speed, our costs and margin across every workspace, the catalog's
/// routes, and which providers are resting on this controller.
async fn operations(
    State(app): State<Shared>,
    Query(input): Query<OperationsQuery>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    if !super::workspace_codes::platform_admin(&app, &user) {
        return Err(Error(
            anyhow::anyhow!("Only Cuckoo administrators see model operations"),
            axum::http::StatusCode::FORBIDDEN,
        ));
    }
    let hours = input.hours.unwrap_or(24);
    if !(1..=24 * 31).contains(&hours) {
        return Err(anyhow::anyhow!("Choose 1 hour to 31 days").into());
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX));
    let mut report = app.store.model_operations(now - hours * 3_600_000).await?;
    report["hours"] = json!(hours);
    report["resting"] = json!(
        router::resting_now()
            .into_iter()
            .map(|(provider, seconds)| json!({"provider": provider, "seconds": seconds}))
            .collect::<Vec<_>>()
    );
    report["catalog"] = match app.store.model_catalog.as_ref().map(catalog::Files::load) {
        None => json!({"on": false, "models": []}),
        Some(Err(error)) => json!({"on": true, "error": format!("{error:#}"), "models": []}),
        Some(Ok(catalog)) => json!({"on": true, "models": catalog.models.iter().map(|m| json!({
            "id": m.id, "name": m.name, "status": m.status, "price": m.price,
            "routes": m.routes.iter().map(|r| json!({"provider": r.provider, "model": r.model, "priority": r.priority,
                "weight": r.weight, "cost": r.cost,
                "margin": {"input": m.price.input - r.cost.input, "output": m.price.output - r.cost.output}})).collect::<Vec<_>>(),
        })).collect::<Vec<_>>()}),
    };
    Ok(Json(report))
}

//! `/api/identity/admin/*` for Cuckoo Control. Every route first checks that the
//! signed-in person is a platform administrator; others get 403 and nothing.
//! Models, routes, prices and the operator model view (`GET /api/identity/admin/models`)
//! come from the model catalog (`identity::models`); this adds what operators change.
use super::super::postgres::operator::{Operator, Via};
use super::super::{ApiResult, App, Error, Shared, identity, workspace_codes::platform_admin};
use super::Deps;
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};

pub(in crate::identity) fn routes() -> Router<Shared> {
    Router::new()
        .route(
            "/control",
            get(|| async { axum::response::Redirect::to("/control.html") }),
        )
        .route("/api/identity/admin/session", get(session))
        .route("/api/identity/admin/overview", get(overview))
        .route("/api/identity/admin/workspaces", get(workspaces))
        .route("/api/identity/admin/workspaces/{id}", get(workspace))
        .route("/api/identity/admin/workspaces/{id}/suspend", post(suspend))
        .route("/api/identity/admin/workspaces/{id}/resume", post(resume))
        .route(
            "/api/identity/admin/providers",
            get(providers).post(add_provider),
        )
        .route("/api/identity/admin/providers/{name}/key", post(store_key))
        .route("/api/identity/admin/providers/{name}/switch", post(switch))
        .route(
            "/api/identity/admin/providers/{name}/remove",
            post(remove_provider),
        )
        .route("/api/identity/admin/spend", get(spend))
        .route(
            "/api/identity/admin/settings",
            get(settings).post(set_setting),
        )
        .route("/api/identity/admin/log", get(log))
        .route("/api/identity/admin/reports", get(reports))
}

/// The person, only if they are a platform administrator.
pub(in crate::identity) async fn operator(app: &App, headers: &HeaderMap) -> ApiResult<Operator> {
    let user = identity(app, headers).await?;
    if !platform_admin(app, &user) {
        return Err(Error(
            anyhow::anyhow!("Only Cuckoo operators can open Cuckoo Control"),
            StatusCode::FORBIDDEN,
        ));
    }
    Ok(Operator {
        user,
        via: Via::Web,
    })
}

fn deps(app: &App) -> Deps<'_> {
    Deps {
        store: &app.store,
        providers_file: app.config.model_providers_file.as_deref(),
        secrets_key_file: app.config.secrets_key_file.as_deref(),
        catalog: app.store.model_catalog.as_ref(),
    }
}

fn reason(text: &str) -> ApiResult<String> {
    Ok(super::super::postgres::operator::reason(text)?)
}

async fn session(State(app): State<Shared>, headers: HeaderMap) -> ApiResult<Json<Value>> {
    let by = operator(&app, &headers).await?;
    Ok(Json(json!({
        "email": by.user.email,
        "name": by.user.display_name.clone().unwrap_or_else(|| by.user.email.clone()),
        "environment": app.config.control_environment,
        "can_store_keys": app.config.secrets_key_file.is_some(),
        "catalog_on": app.store.model_catalog.is_some(),
    })))
}

async fn overview(State(app): State<Shared>, headers: HeaderMap) -> ApiResult<Json<Value>> {
    operator(&app, &headers).await?;
    let mut data = app.store.operator_overview().await?;
    // Live messaging on this server: connections, delivery and sending times.
    let messaging = crate::identity::live_metrics::snapshot();
    let mut extra = Vec::new();
    if messaging["listening"] == false {
        extra.push(json!({"level": "bad", "kind": "listener", "title": "This server isn't hearing message changes",
            "body": "Live messages arrive only through the 30-second check until it reconnects.", "target": null}));
    }
    let delivery = &messaging["delivery"];
    if delivery["count"].as_u64().unwrap_or(0) >= 20
        && delivery["p95_ms"].as_u64().unwrap_or(0) > 1000
    {
        extra.push(json!({"level": "warn", "kind": "delivery", "title": "Live messages are slow to arrive",
            "body": format!("95% of messages reached people within {} ms over the last 10 minutes.", delivery["p95_ms"]), "target": null}));
    }
    data["messaging"] = messaging;
    if let Some(alerts) = data["alerts"].as_array_mut() {
        alerts.extend(extra);
    }
    // Resting is the catalog router's memory on this controller (schema 87).
    for (provider, seconds) in crate::identity::models::router::resting_now() {
        data["alerts"].as_array_mut().into_iter().for_each(|alerts| {
            alerts.push(json!({"level": "warn", "kind": "provider_resting",
                "title": format!("{provider} is resting after a failure"),
                "body": format!("Its routes go last for {seconds} more seconds on this controller."), "target": provider}));
        });
    }
    Ok(Json(data))
}

#[derive(Deserialize)]
struct Search {
    #[serde(default)]
    q: String,
}
async fn workspaces(
    State(app): State<Shared>,
    headers: HeaderMap,
    Query(search): Query<Search>,
) -> ApiResult<Json<Value>> {
    operator(&app, &headers).await?;
    Ok(Json(app.store.operator_workspaces(&search.q).await?))
}

async fn workspace(
    State(app): State<Shared>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    operator(&app, &headers).await?;
    Ok(Json(app.store.operator_workspace(&id).await?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reason {
    reason: String,
}
async fn suspend(
    State(app): State<Shared>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<Reason>,
) -> ApiResult<Json<Value>> {
    let by = operator(&app, &headers).await?;
    app.store
        .operator_suspend(&by, &id, &reason(&input.reason)?)
        .await?;
    Ok(Json(app.store.operator_workspace(&id).await?))
}
async fn resume(
    State(app): State<Shared>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<Reason>,
) -> ApiResult<Json<Value>> {
    let by = operator(&app, &headers).await?;
    app.store
        .operator_resume(&by, &id, &reason(&input.reason)?)
        .await?;
    Ok(Json(app.store.operator_workspace(&id).await?))
}

async fn providers(State(app): State<Shared>, headers: HeaderMap) -> ApiResult<Json<Value>> {
    operator(&app, &headers).await?;
    Ok(Json(deps(&app).providers().await?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewProvider {
    name: String,
    base_url: String,
    key: String,
    reason: String,
}
async fn add_provider(
    State(app): State<Shared>,
    headers: HeaderMap,
    Json(input): Json<NewProvider>,
) -> ApiResult<Json<Value>> {
    let by = operator(&app, &headers).await?;
    deps(&app)
        .add_provider(&by, &input.name, &input.base_url, &input.key, &input.reason)
        .await?;
    Ok(Json(deps(&app).providers().await?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewKey {
    key: String,
    reason: String,
}
async fn store_key(
    State(app): State<Shared>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Json(input): Json<NewKey>,
) -> ApiResult<Json<Value>> {
    let by = operator(&app, &headers).await?;
    deps(&app)
        .store_key(&by, &name, &input.key, &input.reason)
        .await?;
    Ok(Json(deps(&app).providers().await?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Switch {
    enabled: bool,
    reason: String,
}
async fn switch(
    State(app): State<Shared>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Json(input): Json<Switch>,
) -> ApiResult<Json<Value>> {
    let by = operator(&app, &headers).await?;
    deps(&app)
        .switch_provider(&by, &name, input.enabled, &input.reason)
        .await?;
    Ok(Json(deps(&app).providers().await?))
}

async fn remove_provider(
    State(app): State<Shared>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Json(input): Json<Reason>,
) -> ApiResult<Json<Value>> {
    let by = operator(&app, &headers).await?;
    deps(&app)
        .remove_provider(&by, &name, &input.reason)
        .await?;
    Ok(Json(deps(&app).providers().await?))
}

#[derive(Deserialize)]
struct Days {
    #[serde(default = "thirty")]
    days: i64,
}
fn thirty() -> i64 {
    30
}
async fn spend(
    State(app): State<Shared>,
    headers: HeaderMap,
    Query(days): Query<Days>,
) -> ApiResult<Json<Value>> {
    operator(&app, &headers).await?;
    Ok(Json(app.store.operator_spend(days.days).await?))
}

async fn settings(State(app): State<Shared>, headers: HeaderMap) -> ApiResult<Json<Value>> {
    operator(&app, &headers).await?;
    let mut data = app.store.operator_settings().await?;
    data["deployment"] = super::deployment_settings(&app.config);
    Ok(Json(data))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Setting {
    key: String,
    value: Value,
    reason: String,
}
async fn set_setting(
    State(app): State<Shared>,
    headers: HeaderMap,
    Json(input): Json<Setting>,
) -> ApiResult<Json<Value>> {
    let by = operator(&app, &headers).await?;
    app.store
        .operator_set_setting(&by, &input.key, &input.value, &reason(&input.reason)?)
        .await?;
    let mut data = app.store.operator_settings().await?;
    data["deployment"] = super::deployment_settings(&app.config);
    Ok(Json(data))
}

#[derive(Deserialize)]
struct LogQuery {
    area: Option<String>,
    before: Option<i64>,
}
async fn log(
    State(app): State<Shared>,
    headers: HeaderMap,
    Query(q): Query<LogQuery>,
) -> ApiResult<Json<Value>> {
    operator(&app, &headers).await?;
    let area = q.area.as_deref().filter(|a| !a.is_empty() && *a != "all");
    Ok(Json(app.store.operator_log(area, q.before, 50).await?))
}

async fn reports(State(app): State<Shared>, headers: HeaderMap) -> ApiResult<Json<Value>> {
    operator(&app, &headers).await?;
    Ok(Json(
        json!({"reports": app.store.problem_reports(100).await?}),
    ))
}

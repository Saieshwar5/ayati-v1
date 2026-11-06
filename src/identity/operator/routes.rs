//! `/api/identity/admin/*` for Cuckoo Control. Every route first checks that the
//! signed-in person is a platform administrator; others get 403 and nothing.
use super::super::postgres::operator::{
    Operator, Via,
    models::{Route, Target},
};
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
        .route("/api/identity/admin/workspaces/{id}/cap", post(cap))
        .route(
            "/api/identity/admin/providers",
            get(providers).post(add_provider),
        )
        .route("/api/identity/admin/providers/{name}/key", post(rotate_key))
        .route("/api/identity/admin/providers/{name}/switch", post(switch))
        .route(
            "/api/identity/admin/providers/{name}/remove",
            post(remove_provider),
        )
        .route("/api/identity/admin/models", post(set_model))
        .route(
            "/api/identity/admin/routes",
            get(routes_list).post(set_route),
        )
        .route("/api/identity/admin/routes/remove", post(remove_route))
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
    }
}

fn reason(text: &str) -> ApiResult<String> {
    Ok(super::super::postgres::operator::reason(text)?)
}

async fn session(State(app): State<Shared>, headers: HeaderMap) -> ApiResult<Json<Value>> {
    let by = operator(&app, &headers).await?;
    Ok(Json(json!({
        "email": by.user.email, "name": by.user.display_name.clone().unwrap_or_else(|| by.user.email.clone()),
        "environment": app.config.control_environment, "can_store_keys": app.config.secrets_key_file.is_some(),
    })))
}

async fn overview(State(app): State<Shared>, headers: HeaderMap) -> ApiResult<Json<Value>> {
    operator(&app, &headers).await?;
    Ok(Json(app.store.operator_overview().await?))
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Cap {
    /// Dollars per month; null follows the platform default.
    cap: Option<f64>,
    reason: String,
}
async fn cap(
    State(app): State<Shared>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<Cap>,
) -> ApiResult<Json<Value>> {
    let by = operator(&app, &headers).await?;
    app.store
        .operator_set_cap(&by, &id, input.cap, &reason(&input.reason)?)
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
async fn rotate_key(
    State(app): State<Shared>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Json(input): Json<NewKey>,
) -> ApiResult<Json<Value>> {
    let by = operator(&app, &headers).await?;
    deps(&app)
        .rotate_key(&by, &name, &input.key, &input.reason)
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
    app.store
        .operator_remove_provider(&by, &name, &reason(&input.reason)?)
        .await?;
    Ok(Json(deps(&app).providers().await?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Model {
    provider: String,
    model: String,
    enabled: bool,
    input_per_mtok: f64,
    output_per_mtok: f64,
    reason: String,
}
async fn set_model(
    State(app): State<Shared>,
    headers: HeaderMap,
    Json(input): Json<Model>,
) -> ApiResult<Json<Value>> {
    let by = operator(&app, &headers).await?;
    let provider = super::super::postgres::operator::providers::name(&input.provider)?;
    let model = super::super::postgres::operator::models::model_name(&input.model)?;
    app.store
        .operator_set_model(
            &by,
            &provider,
            &model,
            input.enabled,
            input.input_per_mtok,
            input.output_per_mtok,
            &reason(&input.reason)?,
        )
        .await?;
    Ok(Json(deps(&app).providers().await?))
}

async fn routes_list(State(app): State<Shared>, headers: HeaderMap) -> ApiResult<Json<Value>> {
    operator(&app, &headers).await?;
    Ok(Json(app.store.operator_routes().await?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RouteInput {
    model: String,
    main_provider: String,
    main_model: String,
    #[serde(default)]
    second_provider: Option<String>,
    #[serde(default)]
    second_model: Option<String>,
    main_percent: i32,
    timeout_seconds: i32,
    reason: String,
}
impl RouteInput {
    fn route(&self) -> ApiResult<Route> {
        let second = match (
            self.second_provider.as_deref().map(str::trim),
            self.second_model.as_deref().map(str::trim),
        ) {
            (Some(p), Some(m)) if !p.is_empty() && !m.is_empty() => Some(Target {
                provider: p.into(),
                model: m.into(),
            }),
            (None | Some(""), None | Some("")) => None,
            _ => {
                return Err(anyhow::anyhow!(
                    "Name both the second provider and its model, or neither"
                )
                .into());
            }
        };
        Ok(Route {
            model: self.model.trim().into(),
            main: Target {
                provider: self.main_provider.trim().into(),
                model: self.main_model.trim().into(),
            },
            second,
            main_percent: self.main_percent,
            timeout_seconds: self.timeout_seconds,
        })
    }
}
async fn set_route(
    State(app): State<Shared>,
    headers: HeaderMap,
    Json(input): Json<RouteInput>,
) -> ApiResult<Json<Value>> {
    let by = operator(&app, &headers).await?;
    deps(&app)
        .set_route(&by, &input.route()?, &input.reason)
        .await?;
    Ok(Json(app.store.operator_routes().await?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RemoveRoute {
    model: String,
    reason: String,
}
async fn remove_route(
    State(app): State<Shared>,
    headers: HeaderMap,
    Json(input): Json<RemoveRoute>,
) -> ApiResult<Json<Value>> {
    let by = operator(&app, &headers).await?;
    app.store
        .operator_remove_route(&by, input.model.trim(), &reason(&input.reason)?)
        .await?;
    Ok(Json(app.store.operator_routes().await?))
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

//! Shared-workspace authority boundary for supported, reviewed remote MCP operations.
pub(crate) mod broker;
pub(crate) mod catalog;
mod mcp;
pub(crate) mod model;
use super::*;
use anyhow::ensure;
use axum::{Json, extract::Path as RoutePath, http::HeaderMap};
use model::{Decision, Rule};
use serde::Deserialize;
use serde_json::{Value, json};

pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{agent}/control",
            get(snapshot),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{agent}/control/connections",
            post(connect),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{agent}/control/rules",
            post(rule),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{agent}/control/actions/{action}/decision",
            post(decide),
        )
}
fn pg(app: &App) -> Result<&postgres::PgStore> {
    let database::Database::Postgres(pg) = &app.store else {
        anyhow::bail!("Shared tools require PostgreSQL");
    };
    Ok(pg)
}
fn execution_available() -> Result<bool> {
    use super::worker_config::Mode;
    Ok(match Mode::from_env(true)? {
        Mode::Embedded | Mode::Pooled => true,
        Mode::Assigned => std::env::var_os("AYATI_WORKER_CONTROL_URL").is_some(),
        Mode::External => false,
    })
}
async fn snapshot(
    State(app): State<Shared>,
    RoutePath((w, agent)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let mut value = pg(&app)?.control_snapshot(&user, &w, &agent).await?;
    let available = execution_available()?;
    value["execution_available"] = json!(available);
    value["servers"] = if value["can_manage"] == true && available {
        json!(
            catalog::servers(&w)?
                .iter()
                .map(|s| s.public())
                .collect::<Vec<_>>()
        )
    } else {
        json!([])
    };
    Ok(Json(value))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Connection {
    id: String,
    server: String,
    enabled: bool,
}
async fn connect(
    State(app): State<Shared>,
    RoutePath((w, agent)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<Connection>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    pg(&app)?.managed_agent(&user, &w, &agent, true).await?;
    check(
        pg(&app)?.role(&w, &user.id).await? == "Owner",
        "Only workspace owners can connect business tools",
    )?;
    if !input.enabled {
        pg(&app)?
            .control_disconnect(&user, &w, &agent, &input.id)
            .await?;
        return Ok(Json(json!({"saved":true})));
    }
    check(
        execution_available()?,
        "Business tools need controller-managed execution; ask your operator to enable it",
    )?;
    let server = catalog::server(&w, &input.server)?;
    let fingerprint = server.fingerprint()?;
    if input.enabled {
        let mut remote = mcp::Remote::connect(server.clone(), &fingerprint)
            .await
            .map_err(|_| anyhow::anyhow!("Remote MCP connection could not be verified"))?;
        remote.verify(&server.operations).await?;
    }
    pg(&app)?
        .control_connect(&user, &w, &agent, &input.id, server.public(), &fingerprint)
        .await?;
    Ok(Json(json!({"saved":true})))
}
async fn rule(
    State(app): State<Shared>,
    RoutePath((w, agent)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<Rule>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    check(
        input.mode == "block" || execution_available()?,
        "Business tools need controller-managed execution; ask your operator to enable it",
    )?;
    pg(&app)?.control_rule(&user, &w, &agent, &input).await?;
    Ok(Json(json!({"saved":true})))
}
async fn decide(
    State(app): State<Shared>,
    RoutePath((w, agent, action)): RoutePath<(String, String, String)>,
    headers: HeaderMap,
    Json(input): Json<Decision>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let proposed = pg(&app)?
        .control_decide(&user, &w, &agent, &action, &input)
        .await?;
    Ok(Json(dispatch(pg(&app)?, &w, proposed).await?))
}
pub(crate) async fn dispatch(
    pg: &postgres::PgStore,
    w: &str,
    proposed: postgres::control_actions::Proposed,
) -> Result<Value> {
    let Some(dispatch) = proposed.dispatch else {
        return Ok(proposed.action);
    };
    let prepared = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        let server = catalog::server(w, &dispatch.server)?;
        ensure!(
            server.fingerprint()? == dispatch.fingerprint,
            "Connection configuration changed"
        );
        let operation = server.operation(&dispatch.operation.id)?;
        ensure!(
            serde_json::to_value(&operation)? == serde_json::to_value(&dispatch.operation)?,
            "Operation definition changed"
        );
        let mut remote = mcp::Remote::connect(server, &dispatch.fingerprint).await?;
        remote.verify(std::slice::from_ref(&operation)).await?;
        ensure!(
            catalog::server(w, &dispatch.server)?.fingerprint()? == dispatch.fingerprint,
            "Connection changed during setup"
        );
        pg.control_ready(w, &dispatch).await?;
        Ok::<_, anyhow::Error>(remote)
    })
    .await;
    let Ok(Ok(mut remote)) = prepared else {
        return pg.control_preflight_failed(w, &dispatch).await;
    };
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(12),
        remote.call(&dispatch.operation, &dispatch.arguments),
    )
    .await;
    let (value, success) = match result {
        Ok(Ok(value)) => (value, true),
        _ => (
            json!({"message":"The external outcome could not be confirmed. Inspect the remote system before retrying; further writes are held."}),
            false,
        ),
    };
    pg.control_finish(w, &dispatch, value, success).await
}
pub(crate) fn endpoint(input: &str) -> Result<reqwest::Url> {
    let url = reqwest::Url::parse(input).context("Invalid MCP endpoint")?;
    ensure!(
        input.len() <= 2048
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        "MCP endpoint must not contain credentials or query parameters"
    );
    ensure!(
        (url.scheme() == "https" && url.host_str().is_some())
            || (url.scheme() == "http" && url.host_str() == Some("127.0.0.1")),
        "MCP requires HTTPS; explicit loopback HTTP is available for local verification"
    );
    Ok(url)
}

//! Shared-workspace authority boundary for supported, reviewed remote MCP operations.
mod approvals;
pub(crate) mod broker;
pub(crate) mod catalog;
mod connections;
pub(crate) mod connections_model;
pub(crate) mod mcp;
mod mcp_reply;
pub(crate) mod model;
use super::*;
use anyhow::ensure;
use axum::{Json, extract::Path as RoutePath, http::HeaderMap};
use model::{AgentConnection, Decision, Rule};
use serde::Deserialize;
use serde_json::{Value, json};

pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .merge(connections::routes())
        .merge(approvals::routes())
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
            "/api/identity/workspaces/{w}/agent-profiles/{agent}/control/tools",
            post(tools),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{agent}/control/actions/{action}/decision",
            post(decide),
        )
}
/// Business tools reach agents only through pooled executors' worker control.
fn execution_available(app: &App) -> Result<bool> {
    Ok(app.store.pooled)
}
async fn snapshot(
    State(app): State<Shared>,
    RoutePath((w, agent)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let mut value = app.store.control_snapshot(&user, &w, &agent).await?;
    let available = execution_available(&app)?;
    value["execution_available"] = json!(available);
    // Account discovery belongs to the admin-only workspace settings endpoint.
    value["servers"] = json!([]);
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
    app.store.managed_agent(&user, &w, &agent, true).await?;
    check(
        app.store.role(&w, &user.id).await? == "Admin",
        "Only workspace owners can connect business tools",
    )?;
    if !input.enabled {
        app.store
            .control_disconnect(&user, &w, &agent, &input.id)
            .await?;
        return Ok(Json(json!({"saved":true})));
    }
    check(
        execution_available(&app)?,
        "Business tools need controller-managed execution; ask your operator to enable it",
    )?;
    let approved = app.store.workspace_connections(&user, &w).await?;
    check(
        approved["systems"].as_array().is_some_and(|rows| {
            rows.iter()
                .any(|row| row["server"] == input.server && row["approved"] == true)
        }),
        "A workspace admin must approve this system first",
    )?;
    // This older agent-level route connects operator-configured accounts only;
    // accounts that sign in through a provider page connect in Settings.
    let provider = crate::identity::connectors::provider_for(&w, &input.server)?;
    check(
        provider.name() == "mcp",
        "Connect this account in Settings, Connections",
    )?;
    let crate::identity::connectors::Connect::Ready(account) =
        provider.connect(&w, &input.server, "").await?
    else {
        return Err(anyhow::anyhow!("Connect this account in Settings, Connections").into());
    };
    app.store
        .control_connect(
            &user,
            &w,
            &agent,
            &input.id,
            account.definition,
            &account.fingerprint,
        )
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
        input.mode == "block" || execution_available(&app)?,
        "Business tools need controller-managed execution; ask your operator to enable it",
    )?;
    app.store.control_rule(&user, &w, &agent, &input).await?;
    Ok(Json(json!({"saved":true})))
}
async fn tools(
    State(app): State<Shared>,
    RoutePath((w, agent)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<AgentConnection>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    check(
        !input.enabled || execution_available(&app)?,
        "Business tools need controller-managed execution; ask your operator to enable it",
    )?;
    app.store
        .control_agent_connection(&user, &w, &agent, &input)
        .await?;
    Ok(Json(json!({"saved":true})))
}
async fn decide(
    State(app): State<Shared>,
    RoutePath((w, agent, action)): RoutePath<(String, String, String)>,
    headers: HeaderMap,
    Json(input): Json<Decision>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let proposed = app
        .store
        .control_decide(&user, &w, &agent, &action, &input)
        .await?;
    Ok(Json(dispatch(&app.store, &w, proposed).await?))
}
pub(crate) async fn dispatch(
    pg: &postgres::PgStore,
    w: &str,
    proposed: postgres::control_actions::Proposed,
) -> Result<Value> {
    let Some(dispatch) = proposed.dispatch else {
        return Ok(proposed.action);
    };
    match crate::identity::connectors::named(&dispatch.provider) {
        Ok(provider) => dispatch_with(pg, w, dispatch, provider.as_ref()).await,
        Err(_) => {
            pg.control_preflight_failed(
                w,
                &dispatch,
                Some("This account's connector provider is not configured on this server. Nothing was sent."),
            )
            .await
        }
    }
}

/// Sends one claimed action through its provider. Cuckoo's own checks come
/// first (provider switch, exact fields), then the provider's (account still
/// works, action unchanged), then current authority right before sending.
pub(crate) async fn dispatch_with(
    pg: &postgres::PgStore,
    w: &str,
    dispatch: postgres::control_actions::Dispatch,
    provider: &dyn crate::identity::connectors::ConnectorProvider,
) -> Result<Value> {
    use crate::identity::connectors::{Outcome, Refusal, Saved};
    if let Some(note) = pg.connector_provider_paused(provider.name()).await? {
        let reason = format!(
            "Cuckoo paused these connectors{}. Nothing was sent.",
            if note.is_empty() {
                String::new()
            } else {
                format!(": {note}")
            }
        );
        return pg
            .control_preflight_failed(w, &dispatch, Some(&reason))
            .await;
    }
    if dispatch.operation.arguments(&dispatch.arguments).is_err() {
        return pg.control_preflight_failed(w, &dispatch, None).await;
    }
    let saved = Saved {
        workspace: w,
        server: &dispatch.server,
        binding: dispatch.binding.as_ref(),
        fingerprint: &dispatch.fingerprint,
    };
    let prepared = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        let call = provider.prepare(&saved, &dispatch.operation).await?;
        pg.control_ready(w, &dispatch).await?;
        Ok::<_, anyhow::Error>(call)
    })
    .await;
    let call = match prepared {
        Ok(Ok(call)) => call,
        Ok(Err(error)) => {
            let refusal = error.downcast_ref::<Refusal>();
            if matches!(refusal, Some(Refusal::NeedsReconnect)) {
                pg.connector_needs_reconnect(w, &dispatch.connection)
                    .await?;
            }
            let reason = refusal.map(ToString::to_string);
            return pg
                .control_preflight_failed(w, &dispatch, reason.as_deref())
                .await;
        }
        Err(_) => return pg.control_preflight_failed(w, &dispatch, None).await,
    };
    let outcome = tokio::time::timeout(
        std::time::Duration::from_secs(45),
        call.run(&dispatch.arguments),
    )
    .await;
    let (value, success) = match outcome {
        Ok(Outcome::Completed(value)) => (value, true),
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

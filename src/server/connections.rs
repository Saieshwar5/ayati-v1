use super::{ApiResult, WebState};
use anyhow::Context;
use axum::{
    Extension, Json, Router,
    extract::{Path, Query},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

pub(super) fn routes() -> Router<WebState> {
    Router::new()
        .route("/api/connections", get(list).post(link))
        .route("/api/connections/configs", get(configs))
        .route("/api/connections/accounts", get(accounts))
        .route("/api/connections/{id}/refresh", post(refresh))
        .route("/api/connections/{id}/revoke", post(revoke))
        .route("/api/connections/{id}/tools", get(tools))
        .route("/api/connections/{id}/grants", post(grant))
        .route("/api/connector-executions", get(executions))
}
fn platform(state: &WebState) -> anyhow::Result<Arc<crate::platform::Platform>> {
    state
        .agent
        .platform()
        .context("Business services require Goose execution")
}
async fn list(Extension(state): Extension<WebState>) -> ApiResult<Json<Value>> {
    let p = platform(&state)?;
    let setup_error = p.client.as_ref().and_then(|c| c.configuration_error());
    Ok(Json(
        json!({"configured":p.client.is_some() && setup_error.is_none(),"mode":if p.client.as_ref().is_some_and(|c|c.is_consumer()) {"consumer_mcp"} else {"platform"},"setup_error":setup_error,"connections":p.connections()?,"grants":p.grants(state.agent.identity())?}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Link {
    toolkit: String,
    auth_config_id: String,
    label: String,
    account_id: Option<String>,
}
async fn link(
    Extension(state): Extension<WebState>,
    Json(input): Json<Link>,
) -> ApiResult<Json<Value>> {
    let p = platform(&state)?;
    if p.client()?.is_consumer() {
        if input.auth_config_id != "consumer_mcp" {
            return Err(anyhow::anyhow!("Choose a Composio Connect toolkit").into());
        }
        return Ok(Json(
            p.link_consumer(&input.toolkit, &input.label, input.account_id.as_deref())
                .await?,
        ));
    }
    if input.account_id.is_some() {
        return Err(
            anyhow::anyhow!("Select accounts through the configured authentication mode").into(),
        );
    }
    Ok(Json(
        p.link(&input.toolkit, &input.auth_config_id, &input.label)
            .await?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}
async fn refresh(
    Extension(state): Extension<WebState>,
    Path(id): Path<String>,
    Json(_): Json<Empty>,
) -> ApiResult<Json<Value>> {
    platform(&state)?.refresh_connection(&id).await?;
    Ok(Json(json!({"ok":true})))
}
async fn revoke(
    Extension(state): Extension<WebState>,
    Path(id): Path<String>,
    Json(_): Json<Empty>,
) -> ApiResult<Json<Value>> {
    platform(&state)?.revoke_connection(&id).await?;
    Ok(Json(json!({"ok":true})))
}
async fn configs(
    Extension(state): Extension<WebState>,
    Query(search): Query<Search>,
) -> ApiResult<Json<Value>> {
    let p = platform(&state)?;
    if p.client()?.is_consumer() {
        return Ok(Json(
            p.consumer_configs(
                search
                    .query
                    .as_deref()
                    .unwrap_or("Discover Gmail, Google Drive and Google Calendar operations"),
            )
            .await?,
        ));
    }
    let data = p
        .client()?
        .request(reqwest::Method::GET, "auth_configs?limit=100", None)
        .await?;
    let configs = data["items"]
        .as_array()
        .context("Invalid auth configuration list")?
        .iter()
        .map(|v| json!({"id":v["id"],"name":v["name"],"toolkit":v["toolkit"]["slug"]}))
        .collect::<Vec<_>>();
    Ok(Json(json!({"configs":configs})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Search {
    query: Option<String>,
}
async fn tools(
    Extension(state): Extension<WebState>,
    Path(id): Path<String>,
    Query(search): Query<Search>,
) -> ApiResult<Json<Value>> {
    let p = platform(&state)?;
    let connection = p
        .connections()?
        .into_iter()
        .find(|c| c.id == id)
        .context("Unknown connection")?;
    let query = search.query.unwrap_or_default();
    if query.len() > 100 {
        return Err(anyhow::anyhow!("Search is too long").into());
    }
    let data = p.discover_connector_tools(&connection.id, &query).await?;
    let tools=data["items"].as_array().context("Invalid tool list")?.iter().map(|t|json!({"slug":t["slug"],"name":t["name"],"description":t["description"],"version":t["version"]})).collect::<Vec<_>>();
    Ok(Json(
        json!({"tools":tools,"has_more":data["next_cursor"].as_str().is_some_and(|s|!s.is_empty())}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Grant {
    tool: String,
    version: String,
    enabled: bool,
}
async fn grant(
    Extension(state): Extension<WebState>,
    Path(id): Path<String>,
    Json(input): Json<Grant>,
) -> ApiResult<Json<Value>> {
    platform(&state)?
        .set_grant(
            state.agent.identity(),
            &id,
            &input.tool,
            &input.version,
            input.enabled,
        )
        .await?;
    Ok(Json(json!({"ok":true})))
}
async fn executions(Extension(state): Extension<WebState>) -> ApiResult<Json<Value>> {
    Ok(Json(
        json!({"executions":platform(&state)?.executions(state.agent.identity())?}),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Accounts {
    toolkit: String,
}
async fn accounts(
    Extension(state): Extension<WebState>,
    Query(input): Query<Accounts>,
) -> ApiResult<Json<Value>> {
    Ok(Json(
        json!({"accounts":platform(&state)?.client()?.consumer_accounts(&input.toolkit).await?}),
    ))
}

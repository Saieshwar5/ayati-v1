//! The communication gateway: shared message contracts, live transports
//! behind one adapter contract (`adapters`), their background work
//! (`gateway`), and the explicitly local-only email/WhatsApp simulations.
pub(crate) mod adapters;
#[cfg(test)]
mod conformance_tests;
#[cfg(test)]
pub(crate) mod fake_transport;
pub(crate) mod gateway;
pub(crate) mod mcp_transport;
pub(crate) mod model;
use super::*;
use axum::{
    Json,
    extract::{DefaultBodyLimit, Path as RoutePath},
    http::HeaderMap,
};
use model::{Binding, Dispatch, Endpoint, LiveEndpoint, Message, Receipt, SetState};
use serde_json::{Value, json};

pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route("/api/identity/workspaces/{w}/communications",get(status))
        .route("/api/identity/workspaces/{w}/communications/endpoints",post(live_endpoint))
        .route("/api/identity/workspaces/{w}/communications/endpoints/{id}/state",post(live_endpoint_state))
        .route("/api/identity/workspaces/{w}/communications/endpoints/{id}/bindings",post(live_binding))
        .route("/api/identity/workspaces/{w}/communications/endpoints/{id}/bindings/{sender}/state",post(live_binding_state))
        .route("/api/identity/workspaces/{w}/communications/simulation/endpoints",post(endpoint))
        .route("/api/identity/workspaces/{w}/communications/simulation/endpoints/{id}/state",post(endpoint_state))
        .route("/api/identity/workspaces/{w}/communications/simulation/endpoints/{id}/bindings",post(binding))
        .route("/api/identity/workspaces/{w}/communications/simulation/endpoints/{id}/bindings/{sender}/state",post(binding_state))
        .route("/api/identity/workspaces/{w}/communications/simulation/endpoints/{id}/incoming",post(incoming))
        .route("/api/identity/workspaces/{w}/communications/simulation/outbox/{id}/dispatch",post(dispatch))
        .route("/api/identity/workspaces/{w}/communications/simulation/outbox/{id}/receipt",post(receipt))
        .layer(DefaultBodyLimit::max(32*1024))
}
fn simulated(app: &App) -> ApiResult<&postgres::PgStore> {
    if app.simulation.is_none() {
        return Err(Error(
            anyhow::anyhow!("Communication simulation is disabled"),
            StatusCode::NOT_FOUND,
        ));
    }
    Ok(&app.store)
}
async fn status(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let mut status = app.store.communication_status(&user, &w).await?;
    // Which live transports this server offers the workspace; names only.
    status["transports"] = adapters::all()
        .iter()
        .map(|a| json!({"name":a.name(),"servers":a.servers(&w).ok()}))
        .collect();
    Ok(Json(status))
}

/// Each API changes only its own kind of endpoint.
/// Only admins manage endpoints; checked first, so others learn nothing about one.
async fn admin(app: &App, user: &User, w: &str) -> ApiResult<()> {
    if app.store.role(w, &user.id).await.ok().as_deref() != Some("Admin") {
        return Err(Error(
            anyhow::anyhow!("Only workspace admins can manage endpoints"),
            StatusCode::FORBIDDEN,
        ));
    }
    Ok(())
}
async fn kind(app: &App, user: &User, w: &str, id: &str, simulated: bool) -> ApiResult<()> {
    admin(app, user, w).await?;
    if app.store.communication_simulated(w, id).await? != simulated {
        return Err(Error(
            anyhow::anyhow!(
                "Use the {} API for this endpoint",
                if simulated { "live" } else { "simulation" }
            ),
            StatusCode::BAD_REQUEST,
        ));
    }
    Ok(())
}
async fn live_endpoint(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Json(input): Json<LiveEndpoint>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    // Only an admin can make Cuckoo contact a transport server.
    admin(&app, &user, &w).await?;
    let route = adapters::Route {
        workspace: w.clone(),
        server: input.server.clone(),
        address: input.address.clone(),
    };
    adapters::named(&input.transport)?.check(&route).await?;
    Ok(Json(
        app.store
            .communication_live_endpoint(&user, &w, &input)
            .await?,
    ))
}
async fn live_binding(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<Binding>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    kind(&app, &user, &w, &id, false).await?;
    Ok(Json(
        app.store
            .communication_binding(&user, &w, &id, &input)
            .await?,
    ))
}
async fn live_endpoint_state(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<SetState>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    kind(&app, &user, &w, &id, false).await?;
    Ok(Json(
        app.store
            .communication_state(&user, &w, &id, None, &input)
            .await?,
    ))
}
async fn live_binding_state(
    State(app): State<Shared>,
    RoutePath((w, id, sender)): RoutePath<(String, String, String)>,
    headers: HeaderMap,
    Json(input): Json<SetState>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    kind(&app, &user, &w, &id, false).await?;
    Ok(Json(
        app.store
            .communication_state(&user, &w, &id, Some(&sender), &input)
            .await?,
    ))
}
async fn endpoint(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
    Json(input): Json<Endpoint>,
) -> ApiResult<Json<Value>> {
    let pg = simulated(&app)?;
    let user = identity(&app, &headers).await?;
    Ok(Json(pg.communication_endpoint(&user, &w, &input).await?))
}
async fn binding(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<Binding>,
) -> ApiResult<Json<Value>> {
    let pg = simulated(&app)?;
    let user = identity(&app, &headers).await?;
    kind(&app, &user, &w, &id, true).await?;
    Ok(Json(
        pg.communication_binding(&user, &w, &id, &input).await?,
    ))
}
async fn endpoint_state(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<SetState>,
) -> ApiResult<Json<Value>> {
    let pg = simulated(&app)?;
    let user = identity(&app, &headers).await?;
    kind(&app, &user, &w, &id, true).await?;
    Ok(Json(
        pg.communication_state(&user, &w, &id, None, &input).await?,
    ))
}
async fn binding_state(
    State(app): State<Shared>,
    RoutePath((w, id, sender)): RoutePath<(String, String, String)>,
    headers: HeaderMap,
    Json(input): Json<SetState>,
) -> ApiResult<Json<Value>> {
    let pg = simulated(&app)?;
    let user = identity(&app, &headers).await?;
    kind(&app, &user, &w, &id, true).await?;
    Ok(Json(
        pg.communication_state(&user, &w, &id, Some(&sender), &input)
            .await?,
    ))
}
async fn incoming(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<Message>,
) -> ApiResult<Json<Value>> {
    let pg = simulated(&app)?;
    let user = identity(&app, &headers).await?;
    let result = pg.simulate_incoming(&user, &w, &id, &input).await?;
    app.runtimes.chat_wake.notify_one();
    Ok(Json(result))
}
async fn dispatch(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<Dispatch>,
) -> ApiResult<Json<Value>> {
    let pg = simulated(&app)?;
    let user = identity(&app, &headers).await?;
    Ok(Json(
        pg.simulate_dispatch(&user, &w, &id, input.outcome).await?,
    ))
}
async fn receipt(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<Receipt>,
) -> ApiResult<Json<Value>> {
    let pg = simulated(&app)?;
    let user = identity(&app, &headers).await?;
    Ok(Json(pg.simulate_receipt(&user, &w, &id, &input).await?))
}

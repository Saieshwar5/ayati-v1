//! Shared message contracts and explicitly local-only transport simulations.
pub(crate) mod model;
use super::*;
use axum::{
    Json,
    extract::{DefaultBodyLimit, Path as RoutePath},
    http::HeaderMap,
};
use model::{Binding, Dispatch, Endpoint, Message, Receipt, SetState};
use serde_json::Value;

pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route("/api/identity/workspaces/{w}/communications",get(status))
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
    Ok(collaboration::pg(app)?)
}
async fn status(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        collaboration::pg(&app)?
            .communication_status(&user, &w)
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

use super::super::*;
use super::model::Start;
use axum::{
    Json,
    extract::{Path as RoutePath, Query},
    http::HeaderMap,
};
use serde::Deserialize;
use serde_json::{Value, json};
pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{a}/rehearsal-options",
            get(options),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{a}/rehearsals",
            get(history).post(start),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{a}/rehearsals/{id}",
            get(read),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{a}/rehearsals/{id}/stop",
            post(stop),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{a}/rehearsals/{id}/response",
            get(response),
        )
}
fn present(mut data: Value) -> Value {
    if let Some(text) = data["output"].as_str() {
        data["response"] =
            crate::response::reference(data["id"].as_str().unwrap(), text, "complete", 1);
        let text = data["output"].as_str().unwrap();
        data["preview"] = json!(if crate::response::long(text) {
            text.chars()
                .take(crate::response::EXCERPT_CHARS)
                .collect::<String>()
        } else {
            text.to_owned()
        });
    }
    data.as_object_mut().unwrap().remove("output");
    data
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Search {
    #[serde(default)]
    search: String,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct History {
    before: Option<String>,
}
async fn options(
    State(app): State<Shared>,
    RoutePath((w, a)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Query(input): Query<Search>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let mut data = app
        .store
        .rehearsal_options(&user, &w, &a, &input.search)
        .await?;
    data["execution_available"] = json!(available(&app, &data["profile"])?);
    Ok(Json(data))
}
async fn history(
    State(app): State<Shared>,
    RoutePath((w, a)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Query(input): Query<History>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store
            .rehearsal_history(&user, &w, &a, input.before.as_deref())
            .await?,
    ))
}
async fn start(
    State(app): State<Shared>,
    RoutePath((w, a)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<Start>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let pg = &app.store;
    if let Some(saved) = pg.rehearsal_existing(&user, &w, &a, &input).await? {
        return Ok(Json(present(saved)));
    }
    let profile = pg.managed_agent(&user, &w, &a, true).await?;
    check(
        available(&app, &profile)?,
        "The sample model is not configured on this controller. Ask your operator to enable it.",
    )?;
    let result = pg.start_rehearsal(&user, &w, &a, &input).await?;
    app.runtimes.chat_wake.notify_one();
    Ok(Json(present(result)))
}
async fn read(
    State(app): State<Shared>,
    RoutePath((w, a, id)): RoutePath<(String, String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(present(
        app.store.rehearsal(&user, &w, &a, &id).await?,
    )))
}
async fn stop(
    State(app): State<Shared>,
    RoutePath((w, a, id)): RoutePath<(String, String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.stop_rehearsal(&user, &w, &a, &id).await?))
}
async fn response(
    State(app): State<Shared>,
    RoutePath((w, a, id)): RoutePath<(String, String, String)>,
    headers: HeaderMap,
    Query(input): Query<crate::response::Read>,
) -> ApiResult<Response> {
    let user = identity(&app, &headers).await?;
    let data = app.store.rehearsal(&user, &w, &a, &id).await?;
    check(
        data["state"] == "completed",
        "This sample has no completed response",
    )?;
    let text = data["output"]
        .as_str()
        .context("Sample response unavailable")?;
    let body = crate::response::body(&id, text, "complete", 1, input.metadata)?;
    Ok(crate::response::deliver(&id, body, &input))
}

/// Samples run on pooled executors configured for the agent's provider.
fn available(app: &App, profile: &Value) -> Result<bool> {
    let settings: crate::identity::agents::config::Settings =
        serde_json::from_value(profile["settings"].clone())?;
    Ok(crate::identity::hosting::executors_available(
        app,
        &settings.provider,
    ))
}

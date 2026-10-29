use super::super::*;
use super::{
    catalog,
    model::{Change, InboxFull, Trigger},
};
use axum::{
    Json,
    body::Bytes,
    extract::{DefaultBodyLimit, Path as RoutePath},
    http::HeaderMap,
};
use serde_json::{Value, json};
pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{a}/event-triggers",
            get(list).post(create),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{a}/event-triggers/{id}",
            get(details),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{a}/event-triggers/{id}/state",
            post(change),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{a}/event-triggers/{id}/runs",
            get(runs),
        )
        .route(
            "/hooks/composio/workspaces/{w}/{source}",
            post(receive).layer(DefaultBodyLimit::max(64000)),
        )
}
pub(crate) fn webhook_path(path: &str) -> bool {
    let parts: Vec<_> = path.split('/').collect();
    parts.len() == 6
        && parts[0].is_empty()
        && parts[1..4] == ["hooks", "composio", "workspaces"]
        && parts[4..]
            .iter()
            .all(|id| uuid::Uuid::parse_str(id).is_ok())
}
fn sources(w: &str) -> (Vec<catalog::Source>, Option<&'static str>) {
    match catalog::sources(w) {
        Ok(s) => (s, None),
        Err(_) => (
            vec![],
            Some(
                "Operator event-source setup is unavailable. Existing triggers can still be paused.",
            ),
        ),
    }
}
async fn list(
    State(app): State<Shared>,
    RoutePath((w, a)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let (sources, error) = sources(&w);
    let mut data = collaboration::pg(&app)?
        .event_triggers(&user, &w, &a, &sources)
        .await?;
    data["catalog_error"] = json!(error);
    Ok(Json(data))
}
async fn create(
    State(app): State<Shared>,
    RoutePath((w, a)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<Trigger>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let source = catalog::source(&w, &input.source).ok();
    Ok(Json(
        collaboration::pg(&app)?
            .save_event_trigger(&user, &w, &a, &input, source.as_ref())
            .await?,
    ))
}
async fn details(
    State(app): State<Shared>,
    RoutePath((w, a, id)): RoutePath<(String, String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let (sources, _) = sources(&w);
    Ok(Json(
        collaboration::pg(&app)?
            .event_trigger_details(&user, &w, &a, &id, &sources)
            .await?,
    ))
}
async fn change(
    State(app): State<Shared>,
    RoutePath((w, a, id)): RoutePath<(String, String, String)>,
    headers: HeaderMap,
    Json(input): Json<Change>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let (sources, _) = sources(&w);
    Ok(Json(
        collaboration::pg(&app)?
            .change_event_trigger(&user, &w, &a, &id, &input, &sources)
            .await?,
    ))
}
async fn runs(
    State(app): State<Shared>,
    RoutePath((w, a, id)): RoutePath<(String, String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        collaboration::pg(&app)?
            .event_runs(&user, &w, &a, &id)
            .await?,
    ))
}
fn header<'a>(headers: &'a HeaderMap, name: &str) -> ApiResult<&'a str> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            Error(
                anyhow::anyhow!("Webhook authentication required"),
                StatusCode::UNAUTHORIZED,
            )
        })
}
async fn receive(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    let source = catalog::source(&w, &id).map_err(|_| unavailable())?;
    let secret = source.secret().map_err(|_| unavailable())?;
    let event_id = header(&headers, "webhook-id")?;
    super::webhook::verify(
        &secret,
        event_id,
        header(&headers, "webhook-timestamp")?,
        header(&headers, "webhook-signature")?,
        &body,
        store::now(),
    )
    .map_err(|_| {
        Error(
            anyhow::anyhow!("Webhook authentication failed"),
            StatusCode::UNAUTHORIZED,
        )
    })?;
    let payload: Value = serde_json::from_slice(&body).map_err(|_| {
        Error(
            anyhow::anyhow!("Invalid event envelope"),
            StatusCode::BAD_REQUEST,
        )
    })?;
    source.project(event_id, &payload).map_err(|_| {
        Error(
            anyhow::anyhow!("Event identity or reviewed fields do not match"),
            StatusCode::UNAUTHORIZED,
        )
    })?;
    let limit = std::env::var("AYATI_EVENT_RECEIPT_LIMIT")
        .unwrap_or_else(|_| "100000".into())
        .parse::<i64>()
        .ok()
        .filter(|v| (1..=1_000_000).contains(v))
        .ok_or_else(unavailable)?;
    match collaboration::pg(&app)?
        .accept_event(&source, event_id, &payload, limit)
        .await
    {
        Ok(data) => Ok(Json(data)),
        Err(e) if e.is::<InboxFull>() => Err(Error(e, StatusCode::TOO_MANY_REQUESTS)),
        Err(e) if e.to_string() == "Event ID belongs to different content" => {
            Err(Error(e, StatusCode::CONFLICT))
        }
        Err(_) => Err(unavailable()),
    }
}

fn unavailable() -> Error {
    Error(
        anyhow::anyhow!("Event intake is unavailable. Check operator setup and retry."),
        StatusCode::SERVICE_UNAVAILABLE,
    )
}

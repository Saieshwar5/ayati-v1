//! Where a connected system sends its events (C2 of the connected systems
//! plan, D249): `POST /hooks/system/{connection}` with
//! `Authorization: Bearer <events key>`. No cookie authorizes it, and it is one
//! of the narrow exceptions to the browser's Origin checks.
use super::super::*;
use super::{model::InboxFull, system_events::Invalid};
use crate::identity::postgres::systems::events::{Conflict, Refused};
use axum::{
    Json,
    body::Bytes,
    extract::{DefaultBodyLimit, Path as RoutePath},
    http::HeaderMap,
};
use serde_json::Value;

pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route(
            "/hooks/system/{connection}",
            post(receive).layer(DefaultBodyLimit::max(64000)),
        )
        .route("/hooks/system/{connection}/events/{id}", get(runs))
        .route(
            "/hooks/system/{connection}/events/{id}/cancel",
            post(cancel),
        )
}

/// `GET …/events/{id}`: the event's runs, with state and result (C4).
async fn runs(
    State(app): State<Shared>,
    RoutePath((connection, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let key = bearer(&headers).ok_or_else(refused)?;
    match app.store.system_event_runs(&connection, key, &id).await {
        Ok(answer) => Ok(Json(answer)),
        Err(e) if e.is::<Refused>() => Err(refused()),
        Err(e) => Err(Error(e, StatusCode::NOT_FOUND)),
    }
}

/// `POST …/events/{id}/cancel`: stops the event's unfinished runs (C4).
async fn cancel(
    State(app): State<Shared>,
    RoutePath((connection, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let key = bearer(&headers).ok_or_else(refused)?;
    match app.store.cancel_system_event(&connection, key, &id).await {
        Ok(answer) => Ok(Json(answer)),
        Err(e) if e.is::<Refused>() => Err(refused()),
        Err(e) => Err(Error(e, StatusCode::SERVICE_UNAVAILABLE)),
    }
}

fn refused() -> Error {
    Error(
        anyhow::anyhow!("Wrong key, or this system's events are off"),
        StatusCode::UNAUTHORIZED,
    )
}

/// `/hooks/system/<uuid>` and `…/events/<id>/cancel`, for the Origin-check
/// exception. The key authorizes them; no cookie does.
pub(crate) fn hook_path(path: &str) -> bool {
    let parts: Vec<_> = path.split('/').collect();
    let system = parts.len() >= 4
        && parts[0].is_empty()
        && parts[1..3] == ["hooks", "system"]
        && uuid::Uuid::parse_str(parts[3]).is_ok();
    system
        && (parts.len() == 4
            || (parts.len() == 7
                && parts[4] == "events"
                && crate::identity::identifier(parts[5]).is_ok()
                && parts[6] == "cancel"))
}

fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("authorization")?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .filter(|k| !k.is_empty() && k.len() <= 200)
}

async fn receive(
    State(app): State<Shared>,
    RoutePath(connection): RoutePath<String>,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    let key = bearer(&headers).ok_or_else(refused)?;
    let payload: Value = serde_json::from_slice(&body).map_err(|_| {
        Error(
            anyhow::anyhow!("The event must be JSON"),
            StatusCode::UNPROCESSABLE_ENTITY,
        )
    })?;
    match app
        .store
        .system_event(&connection, key, &payload, app.config.event_receipt_limit)
        .await
    {
        Ok(answer) => Ok(Json(answer)),
        Err(e) if e.is::<Refused>() => Err(refused()),
        Err(e) if e.is::<Invalid>() => Err(Error(e, StatusCode::UNPROCESSABLE_ENTITY)),
        Err(e) if e.is::<Conflict>() => Err(Error(e, StatusCode::CONFLICT)),
        Err(e) if e.is::<InboxFull>() => Err(Error(
            anyhow::anyhow!("Too many events at once; send this one again later"),
            StatusCode::TOO_MANY_REQUESTS,
        )),
        Err(e) => {
            tracing::warn!("A connected system's event could not be saved: {e:#}");
            Err(Error(
                anyhow::anyhow!("Event intake is unavailable; send it again later"),
                StatusCode::SERVICE_UNAVAILABLE,
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_a_system_hook_is_exempt() {
        assert!(super::hook_path(
            "/hooks/system/11111111-1111-4111-8111-111111111111"
        ));
        assert!(super::hook_path(
            "/hooks/system/11111111-1111-4111-8111-111111111111/events/evt-1/cancel"
        ));
        for path in [
            "/hooks/system/",
            "/hooks/system/not-a-connection",
            "/hooks/system/11111111-1111-4111-8111-111111111111/x",
            "/hooks/system/11111111-1111-4111-8111-111111111111/events/e1/stop",
        ] {
            assert!(!super::hook_path(path), "{path}");
        }
    }
}

use super::*;
use axum::{
    extract::Query,
    response::sse::{Event, KeepAlive, Sse},
};
use std::{convert::Infallible, time::Duration};

/// Every change record is announced through PostgreSQL NOTIFY (checked by
/// `postgres::messaging_tests`), and the listener wakes every stream when it
/// reconnects, so this re-check only covers a lost notification.
const FALLBACK: Duration = Duration::from_secs(30);

/// Whether a wake-up from the change listener concerns this workspace. `*` is
/// sent when the listener (re)connects: anything announced while it was away.
fn wakes(
    event: &Result<String, tokio::sync::broadcast::error::RecvError>,
    workspace: &str,
) -> bool {
    match event {
        Ok(changed) => changed == workspace || changed == "*",
        // Missed wake-ups (lagging) or a closed channel: check now to be safe.
        Err(_) => true,
    }
}

/// An idle re-check with nothing new is not sent; keep-alives hold the stream.
fn worth_sending(page: &Value, last_cursor: Option<i64>) -> bool {
    last_cursor != page["cursor"].as_i64()
        || page["reset"] == true
        || page["events"]
            .as_array()
            .is_some_and(|events| !events.is_empty())
}
use tokio_stream::wrappers::ReceiverStream;
#[derive(Deserialize)]
struct Cursor {
    after: Option<i64>,
    #[serde(default)]
    compact: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    ids: Vec<String>,
    seen: bool,
}
pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route(
            "/api/identity/workspaces/{workspace}/message-capabilities",
            get(capabilities),
        )
        .route(
            "/api/identity/workspaces/{workspace}/message-events",
            get(events),
        )
        .route("/api/identity/workspaces/{workspace}/unread", get(unread))
        .route(
            "/api/identity/workspaces/{workspace}/chats/{chat}/receipts",
            post(receipt),
        )
        .route(
            "/api/identity/workspaces/{workspace}/chats/{chat}/receipts/{id}",
            get(details),
        )
}
async fn events(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: axum::http::HeaderMap,
    Query(query): Query<Cursor>,
) -> ApiResult<Response> {
    let user = identity(&app, &headers).await?;
    let permit = super::stream_limit::Permit::acquire(&w, &user.id).ok_or_else(|| {
        Error(
            anyhow::anyhow!("Too many live connections. Close an unused tab and retry."),
            StatusCode::TOO_MANY_REQUESTS,
        )
    })?;
    let pg = &app.store;
    super::super::live_metrics::stream_opened();
    let mut wake = pg.changes.subscribe();
    let after = headers
        .get("last-event-id")
        .and_then(|s| s.to_str().ok())
        .and_then(|s| s.parse().ok())
        .or(query.after);
    let mut first = pg.sync_page(&user, &w, after).await?;
    if query.compact {
        crate::response::compact_events(&mut first);
    }
    let (send, receive) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(1);
    tokio::spawn(async move {
        let _permit = permit;
        let mut page = first;
        let mut sent_cursor = None;
        loop {
            let cursor = page["cursor"].as_i64().unwrap_or(0);
            let more = page["more"].as_bool().unwrap_or(false);
            if worth_sending(&page, sent_cursor) {
                let event = Event::default()
                    .event("changes")
                    .id(cursor.to_string())
                    .data(page.to_string());
                if !matches!(
                    tokio::time::timeout(Duration::from_secs(5), send.send(Ok(event))).await,
                    Ok(Ok(()))
                ) {
                    break;
                }
                // Live batches only: the first page is catch-up and may be old.
                if sent_cursor.is_some()
                    && let Some(saved) = page["saved_ms"].as_i64()
                {
                    super::super::live_metrics::delivered(saved);
                }
                sent_cursor = Some(cursor);
            }
            if !more {
                loop {
                    tokio::select! {
                        _=send.closed()=>return,
                        _=app.runtimes.stopping.cancelled()=>return,
                        _=tokio::time::sleep(FALLBACK)=>break,
                        event=wake.recv()=>if wakes(&event, &w) { break }
                    }
                }
            }
            // Revalidate the cookie/session as well as membership on every replay/live batch.
            let result = async {
                // Live batches are reads: a recent WorkOS answer is enough.
                let current = super::super::identity_for_read(&app, &headers).await?;
                let pg = &app.store;
                Ok::<_, Error>(pg.sync_page(&current, &w, Some(cursor)).await?)
            }
            .await;
            match result {
                Ok(mut next) => {
                    if query.compact {
                        crate::response::compact_events(&mut next);
                    }
                    page = next;
                }
                Err(_) => {
                    let _ = send
                        .send(Ok(Event::default().event("unavailable").data("{}")))
                        .await;
                    break;
                }
            }
        }
    });
    Ok((
        [
            ("Cache-Control", "private, no-store"),
            ("X-Accel-Buffering", "no"),
        ],
        Sse::new(ReceiverStream::new(receive))
            .keep_alive(KeepAlive::new().interval(Duration::from_secs(15))),
    )
        .into_response())
}
async fn receipt(
    State(app): State<Shared>,
    RoutePath((w, c)): RoutePath<(String, String)>,
    headers: axum::http::HeaderMap,
    Json(input): Json<Receipt>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    app.store
        .acknowledge(&user, &w, &c, &input.ids, input.seen)
        .await?;
    Ok(Json(json!({"ok":true})))
}
async fn details(
    State(app): State<Shared>,
    RoutePath((w, c, id)): RoutePath<(String, String, String)>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.receipt_details(&user, &w, &c, &id).await?))
}
async fn unread(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.unread(&user, &w).await?))
}
pub(super) fn start(app: Shared) {
    tokio::spawn(async move {
        loop {
            tokio::select! {_=app.runtimes.stopping.cancelled()=>break,_=app.store.listen_changes()=>{}}
            super::super::live_metrics::listener(false);
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });
}

async fn capabilities(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    workspaces::membership(&app, &user, &w).await?;
    Ok(Json(json!({"live":true})))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::broadcast::error::RecvError;

    #[test]
    fn streams_wake_for_their_workspace_a_reconnect_or_a_missed_wake() {
        assert!(wakes(&Ok("w1".into()), "w1"));
        assert!(!wakes(&Ok("w2".into()), "w1"));
        assert!(wakes(&Ok("*".into()), "w1"));
        assert!(wakes(&Err(RecvError::Lagged(3)), "w1"));
        assert!(wakes(&Err(RecvError::Closed), "w1"));
    }

    #[test]
    fn an_idle_check_with_nothing_new_is_not_sent() {
        let idle = json!({"cursor": 7, "more": false, "events": []});
        assert!(worth_sending(&idle, None), "the first page always goes");
        assert!(!worth_sending(&idle, Some(7)));
        assert!(
            worth_sending(&json!({"cursor": 8, "events": []}), Some(7)),
            "progress is saved by the browser"
        );
        assert!(worth_sending(
            &json!({"cursor": 7, "events": [{"kind": "access"}]}),
            Some(7)
        ));
        assert!(worth_sending(
            &json!({"cursor": 7, "reset": true, "events": []}),
            Some(7)
        ));
    }
}

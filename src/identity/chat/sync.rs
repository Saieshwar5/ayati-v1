use super::*;
use crate::identity::postgres::messaging::Live;
use axum::{
    extract::Query,
    response::sse::{Event, KeepAlive, Sse},
};
use std::{convert::Infallible, time::Duration};

/// Every change record is announced through PostgreSQL NOTIFY (checked by
/// `postgres::messaging_tests`), and the listener wakes every stream when it
/// reconnects, so this re-check only covers a lost notification.
const FALLBACK: Duration = Duration::from_secs(30);

type Heard = Result<Live, tokio::sync::broadcast::error::RecvError>;

/// Whether a wake-up from the change listener means this workspace's stream
/// reads its changes. `Everything` is sent when the listener (re)connects:
/// anything announced while it was away. Presence goes out as it is.
fn wakes(event: &Heard, workspace: &str) -> bool {
    match event {
        Ok(Live::Changed(changed)) => changed == workspace,
        Ok(Live::Everything) => true,
        Ok(Live::Presence(_)) => false,
        // Missed wake-ups (lagging) or a closed channel: check now to be safe.
        Err(_) => true,
    }
}

/// Whether presence changes may have been missed, so the stream sends
/// everyone's presence again after its next check.
fn missed_presence(event: &Heard) -> bool {
    matches!(event, Ok(Live::Everything) | Err(_))
}

/// Sent after this long without a batch. It costs no database work.
const KEEP_ALIVE: Duration = Duration::from_secs(15);

/// Browsers fire no event for an SSE comment, so the keep-alive is a named event
/// the tab's watchdog (`web/src/react/chat/live.ts`) can count as a sign of life.
/// Its data must not be empty: browsers drop an event without a data line.
fn keep_alive() -> KeepAlive {
    KeepAlive::new()
        .interval(KEEP_ALIVE)
        .event(Event::default().event("alive").data("{}"))
}

/// An idle re-check with nothing new is not sent; keep-alives hold the stream.
fn worth_sending(page: &Value, last_cursor: Option<i64>) -> bool {
    last_cursor != page["cursor"].as_i64()
        || page["reset"] == true
        || page["events"]
            .as_array()
            .is_some_and(|events| !events.is_empty())
}
/// Sends one event, allowing a slow consumer five seconds; false once the
/// stream is gone or stuck.
async fn deliver(
    send: &tokio::sync::mpsc::Sender<Result<Event, Infallible>>,
    event: Event,
) -> bool {
    matches!(
        tokio::time::timeout(Duration::from_secs(5), send.send(Ok(event))).await,
        Ok(Ok(()))
    )
}

/// Ids and states only (`postgres/presence.rs`); no `id:` line, so the
/// browser's reconnect position stays the change cursor.
fn presence_event(people: String) -> Event {
    Event::default().event("presence").data(people)
}

/// The web build this server hands out: the entry script named in
/// `web/dist/index.html` (`/assets/main-<hash>.js`). A tab built from another
/// entry is out of date and offers to reload (`web/src/react/chat/live.ts`).
/// Read on each stream so a new build is announced once its files are in place.
async fn client_build() -> Option<String> {
    let html = tokio::fs::read_to_string("web/dist/index.html").await.ok()?;
    entry_script(&html)
}
fn entry_script(html: &str) -> Option<String> {
    html.split("<script").skip(1).find_map(|tag| {
        let tag = &tag[..tag.find('>')?];
        if !tag.contains(r#"type="module""#) {
            return None;
        }
        let src = tag.split(r#"src=""#).nth(1)?.split('"').next()?;
        src.starts_with("/assets/").then(|| src.to_owned())
    })
}
/// No `id:` line, like presence, so the reconnect position stays the cursor.
fn version_event(client: Option<String>) -> Event {
    Event::default()
        .event("version")
        .data(json!({"client": client}).to_string())
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
    // A member with this stream open is online (D245).
    super::presence::opened(&app, &w, &user.id).await;
    let (send, receive) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(1);
    let build = client_build().await;
    tokio::spawn(async move {
        let _permit = permit;
        if !deliver(&send, version_event(build)).await {
            return;
        }
        let mut page = first;
        let mut sent_cursor = None;
        // Everyone's presence follows the first batch, and again after a check
        // that may have missed changes; single changes go out as they happen.
        let mut presence_due = true;
        loop {
            let cursor = page["cursor"].as_i64().unwrap_or(0);
            let more = page["more"].as_bool().unwrap_or(false);
            if worth_sending(&page, sent_cursor) {
                let event = Event::default()
                    .event("changes")
                    .id(cursor.to_string())
                    .data(page.to_string());
                if !deliver(&send, event).await {
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
            if presence_due {
                presence_due = false;
                if let Some(people) = super::presence::snapshot(&app, &user, &w).await
                    && !deliver(&send, presence_event(people)).await
                {
                    break;
                }
            }
            if !more {
                // Presence changes go out between checks without one of their own:
                // access was confirmed under FALLBACK ago, and is again by then.
                let check = tokio::time::Instant::now() + FALLBACK;
                loop {
                    tokio::select! {
                        _=send.closed()=>return,
                        _=app.runtimes.stopping.cancelled()=>return,
                        _=tokio::time::sleep_until(check)=>break,
                        event=wake.recv()=>{
                            if let Ok(Live::Presence(change)) = &event
                                && let Some(people) = super::presence::change(change, &w)
                                && !deliver(&send, presence_event(people)).await
                            {
                                return;
                            }
                            presence_due |= missed_presence(&event);
                            if wakes(&event, &w) { break }
                        }
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
        Sse::new(ReceiverStream::new(receive)).keep_alive(keep_alive()),
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
    fn the_client_build_is_the_module_entry_script() {
        let html = r#"<head><script type="module" crossorigin src="/assets/main-DN8.js"></script>
            <link rel="modulepreload" href="/assets/ui-1.js"></head>"#;
        assert_eq!(entry_script(html).as_deref(), Some("/assets/main-DN8.js"));
        // The development server's entry is not a build: no version, no reload prompt.
        assert_eq!(
            entry_script(r#"<script type="module" src="/src/main.tsx"></script>"#),
            None
        );
        assert_eq!(entry_script("<p>no scripts</p>"), None);
    }

    #[test]
    fn streams_wake_for_their_workspace_a_reconnect_or_a_missed_wake() {
        assert!(wakes(&Ok(Live::Changed("w1".into())), "w1"));
        assert!(!wakes(&Ok(Live::Changed("w2".into())), "w1"));
        assert!(wakes(&Ok(Live::Everything), "w1"));
        assert!(wakes(&Err(RecvError::Lagged(3)), "w1"));
        assert!(wakes(&Err(RecvError::Closed), "w1"));
    }

    #[test]
    fn presence_goes_out_as_it_is_and_is_resent_after_anything_missed() {
        use crate::identity::postgres::chat::presence::Change;
        let change = Change {
            workspace: "w1".into(),
            person: json!({"id": "meera", "online": true, "last_seen": 5, "status": null, "revision": 2}),
        };
        let heard: Heard = Ok(Live::Presence(change.into()));
        assert!(
            !wakes(&heard, "w1"),
            "no database read for a presence change"
        );
        let Ok(Live::Presence(change)) = &heard else {
            unreachable!()
        };
        let sent: Value =
            serde_json::from_str(&super::super::presence::change(change, "w1").unwrap()).unwrap();
        assert_eq!(
            sent,
            json!({"people": [{"id": "meera", "online": true, "last_seen": 5, "status": null, "revision": 2}]})
        );
        assert_eq!(
            super::super::presence::change(change, "w2"),
            None,
            "another workspace's"
        );
        assert!(!missed_presence(&heard));
        assert!(!missed_presence(&Ok(Live::Changed("w1".into()))));
        assert!(missed_presence(&Ok(Live::Everything)));
        assert!(missed_presence(&Err(RecvError::Lagged(1))));
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

    #[tokio::test]
    async fn an_idle_stream_sends_a_keep_alive_event_the_browser_can_see() {
        use futures_util::StreamExt;
        let idle = futures_util::stream::pending::<Result<Event, Infallible>>();
        // The production interval is 15 seconds; only the timing is shortened here.
        let response = Sse::new(idle)
            .keep_alive(keep_alive().interval(Duration::from_millis(10)))
            .into_response();
        let mut body = response.into_body().into_data_stream();
        let frame = tokio::time::timeout(Duration::from_secs(5), body.next())
            .await
            .expect("an idle stream sends a keep-alive")
            .expect("the stream stays open")
            .expect("the keep-alive is readable");
        assert_eq!(&frame[..], b"event: alive\ndata: {}\n\n");
    }
}

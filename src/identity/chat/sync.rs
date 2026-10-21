use super::*;
use axum::{
    extract::Query,
    response::sse::{Event, KeepAlive, Sse},
};
use std::{convert::Infallible, time::Duration};
use tokio_stream::wrappers::ReceiverStream;
#[derive(Deserialize)]
struct Cursor {
    after: Option<i64>,
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
    let pg = super::super::collaboration::pg(&app)?;
    let mut wake = pg.changes.subscribe();
    let after = headers
        .get("last-event-id")
        .and_then(|s| s.to_str().ok())
        .and_then(|s| s.parse().ok())
        .or(query.after);
    let first = pg.sync_page(&user, &w, after).await?;
    let (send, receive) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(1);
    tokio::spawn(async move {
        let _permit = permit;
        let mut page = first;
        loop {
            let cursor = page["cursor"].as_i64().unwrap_or(0);
            let more = page["more"].as_bool().unwrap_or(false);
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
            if !more {
                loop {
                    tokio::select! {
                        _=send.closed()=>return,
                        _=app.runtimes.stopping.cancelled()=>return,
                        _=tokio::time::sleep(Duration::from_secs(2))=>break,
                        event=wake.recv()=>match event {Ok(workspace) if workspace!=w=>continue,_=>break}
                    }
                }
            }
            // Revalidate the cookie/session as well as membership on every replay/live batch.
            let result = async {
                let current = identity(&app, &headers).await?;
                let pg = super::super::collaboration::pg(&app)?;
                Ok::<_, Error>(pg.sync_page(&current, &w, Some(cursor)).await?)
            }
            .await;
            match result {
                Ok(next) => page = next,
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
    super::super::collaboration::pg(&app)?
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
    Ok(Json(
        super::super::collaboration::pg(&app)?
            .receipt_details(&user, &w, &c, &id)
            .await?,
    ))
}
async fn unread(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        super::super::collaboration::pg(&app)?
            .unread(&user, &w)
            .await?,
    ))
}
pub(super) fn start(app: Shared) {
    if !matches!(app.store, super::super::database::Database::Postgres(_)) {
        return;
    }
    tokio::spawn(async move {
        loop {
            if let super::super::database::Database::Postgres(pg) = &app.store {
                tokio::select! {_=app.runtimes.stopping.cancelled()=>break,_=pg.listen_changes()=>{}}
            }
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
    Ok(Json(
        json!({"live":matches!(app.store,super::super::database::Database::Postgres(_))}),
    ))
}

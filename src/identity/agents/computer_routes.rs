//! An agent's computer on its Environment tab: its state and this month's use for anyone
//! who can see the agent, and Reset for its owner or an admin. Turning it on and choosing
//! its size and internet are part of the agent's settings. Its browser can be watched
//! live, and used by a person during a hand-over. See runtime/agent-computers.md.
use super::*;
use axum::response::{
    IntoResponse, Response,
    sse::{Event, KeepAlive, Sse},
};

pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{id}/computer",
            get(status),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{id}/computer/reset",
            post(reset),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{id}/computer/live",
            get(live),
        )
        .route(
            "/api/identity/workspaces/{w}/agent-profiles/{id}/computer/live/input",
            post(input),
        )
}

async fn status(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(app.store.computer_status(&user, &w, &id).await?))
}

/// Wipes the computer: the next use starts a new one at the chosen size.
async fn reset(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    app.store.computer_reset(&user, &w, &id).await?;
    Ok(Json(app.store.computer_status(&user, &w, &id).await?))
}

/// The browser's live picture as server-sent events: first whether the viewer controls
/// it, then the latest frame, then each frame and address change as it happens.
async fn live(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let user = identity(&app, &headers).await?;
    let watching = app.store.computer_watch(&user, &w, &id).await?;
    let (send, receive) = tokio::sync::mpsc::channel::<Result<Event, std::convert::Infallible>>(8);
    let feed = watching.id.clone();
    tokio::spawn(async move {
        let mut lines = watching.watch.lines;
        let first = [
            Some(watching.hello.to_string()),
            watching.watch.last.map(|l| l.to_string()),
        ];
        let mut open = true;
        for line in first.into_iter().flatten() {
            open = open && send.send(Ok(Event::default().data(line))).await.is_ok();
        }
        while open {
            match lines.recv().await {
                Ok(line) => {
                    open = send
                        .send(Ok(Event::default().data(&*line)))
                        .await
                        .is_ok()
                }
                // A slow viewer skips frames rather than holding others back.
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    let _ = send
                        .send(Ok(Event::default().event("ended").data("{}")))
                        .await;
                    break;
                }
            }
        }
        app.store.computer_unwatch(&feed);
    });
    Ok((
        [
            ("Cache-Control", "private, no-store"),
            ("X-Accel-Buffering", "no"),
        ],
        Sse::new(tokio_stream::wrappers::ReceiverStream::new(receive))
            .keep_alive(KeepAlive::new().interval(std::time::Duration::from_secs(15))),
    )
        .into_response())
}

/// A person's mouse, scroll, typing or key during their hand-over.
async fn input(
    State(app): State<Shared>,
    RoutePath((w, id)): RoutePath<(String, String)>,
    headers: HeaderMap,
    Json(event): Json<crate::identity::postgres::LiveInput>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    Ok(Json(
        app.store
            .computer_live_input(&user, &w, &id, &event)
            .await?,
    ))
}

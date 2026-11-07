//! Setting your status, and the two controller loops that keep presence
//! current (D245; storage and privacy in `postgres/presence.rs`).
//!
//! Presence needs no browser timer: every live stream open on this controller
//! keeps its person online through one beat for everybody every `BEAT`, and
//! the sweep takes people offline once their last beat expires. Both loops do
//! no database work while nobody is connected.
use super::*;
use crate::identity::postgres::chat::presence::{self, TTL_MS, now_ms};
use std::time::Duration;
use tokio::time::Instant;

/// How often this controller records a beat for the people connected to it.
/// The store skips beats under 28 seconds apart, so this writes once in 30.
const BEAT: Duration = Duration::from_secs(15);
/// Shortest time between two sweeps, however many people expire together.
const SWEEP_GAP: Duration = Duration::from_secs(1);
/// After a failed sweep.
const SWEEP_RETRY: Duration = Duration::from_secs(30);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SetStatus {
    /// None (or left out) clears the status, which shows you as Available.
    status: Option<status::Status>,
}

pub(super) fn routes() -> Router<Shared> {
    Router::new().route(
        "/api/identity/workspaces/{workspace}/status",
        post(set_status),
    )
}

async fn set_status(
    State(app): State<Shared>,
    RoutePath(w): RoutePath<String>,
    headers: axum::http::HeaderMap,
    Json(input): Json<SetStatus>,
) -> ApiResult<Json<Value>> {
    let user = identity(&app, &headers).await?;
    let now = now_ms();
    let status = input.status.map(|status| status.checked(now)).transpose()?;
    Ok(Json(
        app.store
            .set_status(&user, &w, status.as_ref(), now)
            .await?,
    ))
}

/// Records at once that someone just opened a live stream, so they show as
/// online before the next round of beats.
pub(super) async fn opened(app: &Shared, workspace: &str, user: &str) {
    let person = [(workspace.to_owned(), user.to_owned())];
    if app.store.beat_presence(&person, now_ms()).await.is_err() {
        tracing::warn!("Presence beat unavailable");
    }
}

pub(super) fn start(app: Shared) {
    let beats = app.clone();
    tokio::spawn(async move {
        let stopping = beats.runtimes.stopping.clone();
        loop {
            tokio::select! {
                _ = stopping.cancelled() => break,
                _ = tokio::time::sleep(BEAT) => {}
            }
            let people = super::stream_limit::open_people();
            if !people.is_empty() && beats.store.beat_presence(&people, now_ms()).await.is_err() {
                tracing::warn!("Presence beats unavailable");
            }
        }
    });
    tokio::spawn(async move {
        let stopping = app.runtimes.stopping.clone();
        let work = app.store.work.clone();
        // Sweep once at start: people may have expired while no controller ran.
        let mut due = Some(Instant::now());
        loop {
            let wait = async {
                match due {
                    Some(at) => tokio::time::sleep_until(at).await,
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                _ = stopping.cancelled() => break,
                _ = work.presence.notified() => {
                    // Someone came online; the earliest they can expire is a TTL away.
                    let at = Instant::now() + Duration::from_millis(TTL_MS as u64);
                    due = Some(due.map_or(at, |due| due.min(at)));
                    continue;
                }
                _ = wait => {}
            }
            due = match app.store.expire_presence(now_ms()).await {
                Ok(next) => next.map(|next| Instant::now() + next.max(SWEEP_GAP)),
                Err(_) => {
                    tracing::warn!("Presence sweep unavailable");
                    Some(Instant::now() + SWEEP_RETRY)
                }
            };
        }
    });
}

/// Everyone's presence for a stream that just connected or may have missed
/// changes; None when it cannot be read now (the next resynchronisation retries).
pub(super) async fn snapshot(app: &Shared, user: &User, workspace: &str) -> Option<String> {
    let people = app.store.presence(user, workspace, now_ms()).await.ok()?;
    Some(json!({"people": people}).to_string())
}

/// One person's change for the streams of their workspace.
pub(super) fn change(change: &presence::Change, workspace: &str) -> Option<String> {
    (change.workspace == workspace).then(|| json!({"people": [change.person]}).to_string())
}

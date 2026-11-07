//! Reads chat files once for agents (schema 100).
//!
//! Every controller runs this worker. A database lease decides which controller
//! reads each file, so replicas never read the same one twice. It uses the
//! Library's local readers in their sandbox (no network, limited memory, CPU and
//! time), so no file or text leaves the controller and no setting is needed.
//! Where a reader is missing (`pdftotext`, `bwrap`, `tesseract`), the file is
//! marked unsupported and stays downloadable. Neither bytes nor text are logged.
#[cfg(test)]
mod worker_tests;

use super::{
    Shared,
    collaboration::Object,
    library::extract,
    postgres::objects::contents::{Claimed, Outcome},
};
use std::time::Duration;

/// The upload limit; nothing larger is ever read.
const MAX_BYTES: i64 = 10 * 1024 * 1024;
/// Files one controller reads at once. Text recognition can use a CPU and a
/// gigabyte of memory for 25 seconds, so this stays small.
const AT_ONCE: i64 = 2;
/// An upload wakes the worker through PostgreSQL; it still looks this often in
/// case a notification was lost.
const FALLBACK: Duration = Duration::from_secs(60);
/// After the queue was unreadable, look again this soon.
const RETRY: Duration = Duration::from_secs(5);

static WAKE: tokio::sync::Notify = tokio::sync::Notify::const_new();

/// Starts this controller's worker now rather than at its next look.
pub(crate) fn wake() {
    WAKE.notify_one();
}

pub(in crate::identity) fn start(app: Shared) {
    tokio::spawn(async move {
        let stopping = app.runtimes.stopping.clone();
        let work = app.store.work.clone();
        loop {
            let busy = round(&app).await;
            let pause = if busy {
                Duration::ZERO
            } else {
                match app.store.next_reading_in().await {
                    Ok(next) => next.map_or(FALLBACK, |next| next.min(FALLBACK)),
                    Err(_) => RETRY,
                }
            };
            tokio::select! {
                _ = stopping.cancelled() => break,
                _ = WAKE.notified() => {}
                _ = work.read.notified() => {}
                _ = tokio::time::sleep(pause) => {}
            }
        }
    });
}

/// Takes a few waiting files and reads them together; true when there were any.
pub(crate) async fn round(app: &Shared) -> bool {
    let claimed = match app.store.claim_readings(AT_ONCE).await {
        Ok(claimed) => claimed,
        Err(_) => {
            tracing::warn!("Chat file reading queue unavailable");
            return false;
        }
    };
    let finished = claimed.iter().map(|item| async move {
        let outcome = read(app, item).await;
        if app.store.finish_reading(item, outcome).await.is_err() {
            tracing::warn!("Chat file reading could not be saved; it will be tried again");
        }
    });
    futures_util::future::join_all(finished).await;
    !claimed.is_empty()
}

async fn read(app: &Shared, item: &Claimed) -> Outcome {
    if item.size > MAX_BYTES {
        return Outcome::Unreadable { kind: None };
    }
    let object = Object {
        id: item.object.clone(),
        name: item.name.clone(),
        mime: item.mime.clone(),
        size: item.size,
        checksum: item.checksum.clone(),
    };
    let Ok(bytes) = super::collaboration::stored_object(app, &item.workspace, &object).await else {
        tracing::warn!("Chat file unavailable from file storage; it will be tried again");
        return Outcome::Retry;
    };
    outcome(&bytes).await
}

/// What the readers find in a file's bytes. Its type is detected again from the
/// bytes, so files recorded before schema 100 as opaque bytes are read too.
pub(crate) async fn outcome(bytes: &axum::body::Bytes) -> Outcome {
    let mime = extract::mime(bytes);
    let Some(kind) = extract::kind(mime) else {
        return Outcome::Unreadable { kind: None };
    };
    let (text, readiness) = extract::text(bytes, mime).await;
    if readiness == "unsupported" || text.trim().is_empty() {
        return Outcome::Unreadable { kind: Some(kind) };
    }
    // Text, Word and Excel say "partial" when their text was cut short; PDFs and
    // images always do (a PDF is read to page 50, recognition may be wrong).
    let truncated = readiness == "partial" && matches!(kind, "text" | "word" | "excel");
    Outcome::Read {
        kind,
        text,
        truncated,
    }
}

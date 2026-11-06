//! The keeping sweep (LB7): a controller task that deletes, for good, Library items past
//! their folder's keeping rule, then removes their stored files, and removes the stored
//! bytes of uploads that were cancelled or never finished. Two controllers may sweep at
//! once; each item is purged once, under a row lock.
use crate::identity::Shared;
use std::time::Duration;

pub(in crate::identity) fn start(app: Shared) {
    let every = Duration::from_secs(app.config.library.keeping_seconds);
    tokio::spawn(async move {
        let stopping = app.runtimes.stopping.clone();
        let mut tick = tokio::time::interval(every);
        loop {
            tokio::select! {
                _ = stopping.cancelled() => break,
                _ = tick.tick() => {}
            }
            sweep(&app).await;
        }
    });
}

async fn sweep(app: &Shared) {
    let at = crate::identity::now();
    match app.store.library_keeping_sweep(at).await {
        Ok(files) => remove(app, files).await,
        Err(error) => tracing::warn!("The Library keeping sweep failed; it will retry: {error}"),
    }
    match app.store.library_orphaned_uploads(at).await {
        Ok(files) => remove(app, files).await,
        Err(error) => {
            tracing::warn!("Cleaning unfinished Library uploads failed; it will retry: {error}")
        }
    }
}

/// Removes stored files whose rows are already gone. A file left behind is unreachable.
pub(in crate::identity) async fn remove(app: &Shared, files: Vec<String>) {
    for key in files {
        if let Err(error) = app.library.delete(&key).await {
            tracing::warn!("A deleted Library file could not be removed from storage: {error}");
        }
    }
}

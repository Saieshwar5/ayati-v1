//! The keeping sweep (LB7): a controller task that deletes, for good, Library items past
//! their folder's keeping rule, then removes their stored files. Two controllers may
//! sweep at once; each item is purged once, under a row lock.
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
    match app
        .store
        .library_keeping_sweep(crate::identity::now())
        .await
    {
        Ok(files) => {
            for key in files {
                // The rows are gone; a file left behind is unreachable and retried never.
                if let Err(error) = app.library.delete(&key).await {
                    tracing::warn!(
                        "A deleted Library file could not be removed from storage: {error}"
                    );
                }
            }
        }
        Err(error) => tracing::warn!("The Library keeping sweep failed; it will retry: {error}"),
    }
}

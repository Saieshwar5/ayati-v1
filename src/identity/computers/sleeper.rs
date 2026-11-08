//! The sleep sweep: every half minute, idle agent computers go to sleep (and their awake
//! time is charged), computers being reset or whose agent is gone are deleted. Several
//! controllers may sweep; a computer already asleep is left alone.
use crate::identity::Shared;
use std::time::Duration;

pub(in crate::identity) fn start(app: Shared) {
    if app.store.computers.is_none() {
        return;
    }
    tokio::spawn(async move {
        let stopping = app.runtimes.stopping.clone();
        let mut tick = tokio::time::interval(Duration::from_secs(30));
        loop {
            tokio::select! {
                _ = stopping.cancelled() => break,
                _ = tick.tick() => {}
            }
            if let Err(error) = app.store.computers_sweep().await {
                tracing::warn!("The agent computer sweep failed; it will retry: {error}");
            }
        }
    });
}

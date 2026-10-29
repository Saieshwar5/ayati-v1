//! Background removal of duplicates and delivery notices (D226). History is
//! never removed here. Runs every few minutes on every controller; a database
//! lock lets only one controller clean at a time, in small batches.
use super::Shared;
use std::time::Duration;

const EVERY: Duration = Duration::from_secs(300);
const BATCH: i64 = 500;
/// Batches per round, so one round never runs long.
const ROUNDS: usize = 20;
/// Leave a just-stopped reply's chunks briefly for late, refused output retries.
const SETTLE_SECONDS: i64 = 600;

pub(super) fn start(app: Shared) {
    tokio::spawn(async move {
        let stopping = app.runtimes.stopping.clone();
        let mut tick = tokio::time::interval(EVERY);
        loop {
            tokio::select! {
                _ = stopping.cancelled() => break,
                _ = tick.tick() => {}
            }
            let (mut replies, mut notices) = (0, 0);
            for _ in 0..ROUNDS {
                let compacted = app.store.compact_replies(SETTLE_SECONDS, BATCH).await;
                let expired = app
                    .store
                    .expire_sync_events(app.config.retain_sync_days, BATCH)
                    .await;
                if compacted.is_err() || expired.is_err() {
                    tracing::warn!("Data cleanup unavailable; it will retry");
                }
                let (compacted, expired) = (compacted.unwrap_or(0), expired.unwrap_or(0));
                replies += compacted;
                notices += expired;
                // Continue only while a batch came back full.
                if compacted < BATCH as u64 && expired < BATCH as u64 {
                    break;
                }
            }
            if replies + notices > 0 {
                tracing::info!(
                    replies,
                    notices,
                    "Removed duplicated reply chunks and old live-update notices"
                );
            }
        }
    });
}

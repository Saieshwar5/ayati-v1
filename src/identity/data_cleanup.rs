//! Background removal of duplicates and delivery notices, and archiving of old
//! attempt diagnostics to cold storage (D226). History is never removed here.
//! Runs every few minutes on every controller; a database lock lets only one
//! controller clean at a time, in small batches.
use super::Shared;
use std::time::Duration;

const EVERY: Duration = Duration::from_secs(300);
const BATCH: i64 = 500;
/// Batches per round, so one round never runs long.
const ROUNDS: usize = 20;
/// Leave a just-stopped reply's chunks briefly for late, refused output retries.
const SETTLE_SECONDS: i64 = 600;

pub(super) fn start(app: Shared) {
    // Without an archive, attempt history stays in the database.
    let archive = match super::archive::Archive::from_settings(&app.config.archive) {
        Ok(archive) => archive,
        Err(_) => {
            tracing::warn!("Archive unavailable; attempt history stays in the database");
            None
        }
    };
    tokio::spawn(async move {
        let stopping = app.runtimes.stopping.clone();
        let mut tick = tokio::time::interval(EVERY);
        loop {
            tokio::select! {
                _ = stopping.cancelled() => break,
                _ = tick.tick() => {}
            }
            // Deletions whose 7-day undo has passed close the workspace.
            if app.store.close_deleted_workspaces(20).await.is_err() {
                tracing::warn!("Closing deleted workspaces failed; it will retry");
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
            if let Some(archive) = &archive {
                match archive_attempts(&app.store, app.config.archive.attempt_days, archive).await {
                    Ok(0) => {}
                    Ok(archived) => tracing::info!(archived, "Archived old attempt history"),
                    Err(_) => tracing::warn!(
                        "Attempt archiving unavailable; history stays in the database"
                    ),
                }
            }
        }
    });
}

/// Archive finished attempts older than the retention period, then remove them.
/// Each batch is stored before its rows are deleted; nothing is lost on a crash.
pub(crate) async fn archive_attempts(
    store: &super::postgres::PgStore,
    days: i64,
    archive: &super::archive::Archive,
) -> anyhow::Result<u64> {
    let mut archived = 0;
    for _ in 0..ROUNDS {
        let records = store.stale_attempts(days, BATCH).await?;
        let (Some(first), Some(last)) = (records.first(), records.last()) else {
            break;
        };
        let key = format!(
            "attempts/{}/{}-{}.jsonl.gz",
            first["archive_day"].as_str().unwrap_or("undated"),
            first["id"].as_str().unwrap_or_default(),
            last["id"].as_str().unwrap_or_default()
        );
        archive.put(&key, &records).await?;
        let ids: Vec<String> = records
            .iter()
            .filter_map(|record| record["id"].as_str().map(str::to_owned))
            .collect();
        let removed = store.remove_attempts(&ids).await?;
        archived += removed;
        // Another controller holds the cleanup lock: it removes this batch.
        if removed == 0 || (records.len() as i64) < BATCH {
            break;
        }
    }
    Ok(archived)
}

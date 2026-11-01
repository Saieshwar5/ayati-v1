//! The gateway's background work, every few seconds on each controller: read
//! new events from every live transport, send replies waiting in the outbox
//! and ask about sends whose outcome was unclear. Several controllers can run
//! it at once: rows are claimed with `SKIP LOCKED` and repeats are deduplicated.
use super::adapters::{Event, Sent, TransportAdapter};
use crate::identity::postgres::PgStore;
use std::{sync::Arc, time::Duration};

const EVERY: Duration = Duration::from_secs(3);
const BATCH: i64 = 20;

pub(crate) fn start(app: super::super::Shared) {
    tokio::spawn(async move {
        let stopping = app.runtimes.stopping.clone();
        let mut tick = tokio::time::interval(EVERY);
        let mut was_unreachable = false;
        loop {
            tokio::select! {
                _ = stopping.cancelled() => break,
                _ = tick.tick() => {}
            }
            let round = run_once(&app.store, &super::adapters::all()).await;
            if round.unreachable > 0 && !was_unreachable {
                tracing::warn!(
                    servers = round.unreachable,
                    "A transport server could not be reached; it will retry"
                );
            }
            was_unreachable = round.unreachable > 0;
            if round.received + round.sent + round.settled + round.skipped > 0 {
                tracing::info!(
                    round.received,
                    round.sent,
                    round.settled,
                    round.skipped,
                    "Communication gateway"
                );
            }
        }
    });
}

/// What one round did, for logs and tests. Counts only, never content.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Round {
    pub received: usize,
    pub skipped: usize,
    pub sent: usize,
    pub settled: usize,
    pub unreachable: usize,
}

pub(crate) async fn run_once(store: &PgStore, adapters: &[Arc<dyn TransportAdapter>]) -> Round {
    let mut round = Round::default();
    receive(store, adapters, &mut round).await;
    deliver(store, adapters, &mut round).await;
    settle(store, adapters, &mut round).await;
    round
}

fn adapter<'a>(
    adapters: &'a [Arc<dyn TransportAdapter>],
    name: &str,
) -> Option<&'a Arc<dyn TransportAdapter>> {
    adapters.iter().find(|a| a.name() == name)
}

/// A database failure, after which the batch is read again. Anything else is
/// Cuckoo refusing the event (unknown address, a changed retry), so it is dropped.
fn transient(error: &anyhow::Error) -> bool {
    error.chain().any(|e| e.is::<sea_orm::DbErr>())
}

async fn receive(store: &PgStore, adapters: &[Arc<dyn TransportAdapter>], round: &mut Round) {
    let Ok(sources) = store.transport_sources().await else {
        return;
    };
    for (source, cursor) in sources {
        let Some(adapter) = adapter(adapters, &source.transport) else {
            continue;
        };
        let Ok(batch) = adapter.fetch(&source, cursor.as_deref()).await else {
            round.unreachable += 1;
            continue;
        };
        round.skipped += batch.skipped;
        let mut complete = true;
        for event in &batch.events {
            let saved =
                match event {
                    Event::Message(message) => store
                        .transport_incoming(&source, message)
                        .await
                        .map(|saved| {
                            if saved["duplicate"] == false {
                                round.received += 1;
                            }
                        }),
                    Event::Receipt { key, delivered } => {
                        store.transport_receipt(&source, key, *delivered).await
                    }
                };
            match saved {
                Err(error) if transient(&error) => {
                    complete = false;
                    break;
                }
                Err(_) => round.skipped += 1,
                Ok(()) => {}
            }
        }
        // Events are saved before the cursor moves; a crash rereads them.
        if complete
            && let Some(next) = batch
                .next
                .as_deref()
                .filter(|n| Some(*n) != cursor.as_deref())
            && store
                .save_transport_cursor(&source, cursor.as_deref(), next)
                .await
                .is_err()
        {
            tracing::warn!("A transport cursor could not be saved; its events will be read again");
        }
    }
}

async fn deliver(store: &PgStore, adapters: &[Arc<dyn TransportAdapter>], round: &mut Round) {
    let Ok(claimed) = store.claim_deliveries(BATCH).await else {
        return;
    };
    for delivery in claimed {
        let sent = match adapter(adapters, &delivery.transport) {
            Some(adapter) => adapter.send(&delivery.route, &delivery.outgoing).await,
            None => Sent::Unreached("This transport is not configured on this server".into()),
        };
        if matches!(sent, Sent::Unreached(_)) {
            round.unreachable += 1;
        } else {
            round.sent += 1;
        }
        // If this fails the lease expires and the send is treated as unclear.
        if store.finish_delivery(&delivery, &sent).await.is_err() {
            tracing::warn!("A send's outcome could not be saved; it will be checked again");
        }
    }
}

async fn settle(store: &PgStore, adapters: &[Arc<dyn TransportAdapter>], round: &mut Round) {
    let Ok(due) = store.uncertain_deliveries(BATCH).await else {
        return;
    };
    for delivery in due {
        let Some(adapter) = adapter(adapters, &delivery.transport) else {
            continue;
        };
        let answer = adapter.status(&delivery.route, &delivery.attempt).await;
        let known = matches!(
            answer,
            Sent::Accepted(_) | Sent::Delivered(_) | Sent::Refused(_)
        );
        if store.settle_delivery(&delivery, &answer).await.is_ok() && known {
            round.settled += 1;
        }
    }
}

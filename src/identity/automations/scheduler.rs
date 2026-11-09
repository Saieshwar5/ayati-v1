use super::super::*;
use std::time::Duration;

/// Rounds start at most this often however often work is announced, as when the
/// scheduler polled every second (until November 2026, about 15 transactions a
/// second on an idle controller).
const MIN_GAP: Duration = Duration::from_secs(1);
/// Without an announcement or a due time, look again after this long in case a
/// notification was lost or something changed that announces nothing.
const FALLBACK: Duration = Duration::from_secs(30);

pub(crate) async fn start(app: Shared) {
    let pg = &app.store;
    if !pg.pooled_execution() {
        return;
    }
    let owner = app.clone();
    app.runtimes
        .automation_tasks
        .lock()
        .await
        .spawn(async move {
            let pg = &owner.store;
            let stopping = owner.runtimes.stopping.clone();
            loop {
                let started = tokio::time::Instant::now();
                let mut busy = false;
                for result in [
                    pg.dispatch_schedules().await,
                    pg.dispatch_events().await,
                    crate::identity::events::subscriptions::sweep(pg).await,
                    pg.dispatch_library_arrivals().await,
                    pg.dispatch_app_events().await,
                    pg.reconcile_handoffs().await,
                    pg.dispatch_task_wakeups().await,
                    pg.dispatch_panel_updates().await,
                    crate::identity::home_panels::direct::run(&owner).await,
                ] {
                    match result {
                        Ok(n) if n > 0 => {
                            busy = true;
                            owner.runtimes.chat_wake.notify_one();
                        }
                        Ok(_) => {}
                        Err(_) => {
                            busy = true;
                            tracing::warn!(
                                "Shared work admission unavailable; accepted work remains saved"
                            );
                        }
                    }
                }
                // After admitting work (a batch may have had more) or an error, look
                // again soon; otherwise sleep until the next schedule, follow-up,
                // panel update or helper deadline is due.
                let until = if busy {
                    started + MIN_GAP
                } else {
                    match pg.next_admission_in().await {
                        Ok(next) => {
                            let wait = next.map_or(FALLBACK, |next| next.min(FALLBACK));
                            tokio::time::Instant::now() + wait
                        }
                        Err(_) => started + MIN_GAP,
                    }
                };
                tokio::select! {
                    _ = stopping.cancelled() => break,
                    _ = pg.work.schedule.notified() => {}
                    _ = tokio::time::sleep_until(until) => {}
                }
                tokio::select! {
                    _ = stopping.cancelled() => break,
                    _ = tokio::time::sleep_until(started + MIN_GAP) => {}
                }
            }
        });
}

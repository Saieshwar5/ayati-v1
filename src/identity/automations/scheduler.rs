use super::super::*;
pub(crate) async fn start(app: Shared) {
    let database::Database::Postgres(pg) = &app.store else {
        return;
    };
    if !pg.pooled_execution() {
        return;
    }
    let owner = app.clone();
    app.runtimes
        .automation_tasks
        .lock()
        .await
        .spawn(async move {
            let database::Database::Postgres(pg) = &owner.store else {
                return;
            };
            loop {
                tokio::select! {
                    _=owner.runtimes.stopping.cancelled()=>break,
                    _=tokio::time::sleep(std::time::Duration::from_secs(1))=>{},
                }
                for result in [
                    pg.dispatch_schedules().await,
                    pg.dispatch_events().await,
                    pg.reconcile_handoffs().await,
                ] {
                    match result {
                        Ok(n) if n > 0 => owner.runtimes.chat_wake.notify_one(),
                        Ok(_) => {}
                        Err(_) => tracing::warn!(
                            "Shared work admission unavailable; accepted work remains saved"
                        ),
                    }
                }
            }
        });
}

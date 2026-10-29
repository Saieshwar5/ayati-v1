use super::Agent;
use crate::storage::Task;
use anyhow::Result;
use std::sync::{Arc, atomic::Ordering};
use tokio_util::sync::CancellationToken;

impl Agent {
    pub(super) fn start_worker(agent: Arc<Self>) {
        tokio::spawn(async move {
            loop {
                let notified = agent.wake.notified();
                let environment = agent.environment.lock().await;
                // Claim and register cancellation under the same local lock as
                // Stop/shutdown. SQLite, not Notify, owns accepted work.
                let next = (|| -> Result<_> {
                    let mut active = agent
                        .cancellation
                        .lock()
                        .map_err(|_| anyhow::anyhow!("Agent state unavailable"))?;
                    if agent.closing.load(Ordering::SeqCst) {
                        return Ok(None);
                    }
                    let claim = agent.store.claim_next(&agent.owner_id)?;
                    Ok(claim.map(|claim| {
                        let cancel = CancellationToken::new();
                        active.insert(claim.task_id.clone(), cancel.clone());
                        (claim, cancel)
                    }))
                })();
                let next = match next {
                    Ok(value) => value,
                    Err(error) => {
                        // Persisted jobs remain queued if the DB temporarily fails.
                        tracing::error!(%error, "Could not claim saved work");
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        continue;
                    }
                };
                if agent.closing.load(Ordering::SeqCst) && next.is_none() {
                    break;
                }
                let Some((claim, cancel)) = next else {
                    drop(environment);
                    tokio::select! {
                        _ = notified => {},
                        _ = agent.shutdown.cancelled() => {},
                    }
                    continue;
                };
                let result = agent.run(&claim, cancel.clone()).await;
                let status = if cancel.is_cancelled() {
                    "stopped"
                } else if result.is_ok() {
                    "completed"
                } else {
                    "failed"
                };
                let detail = result
                    .err()
                    .map(|error| format!("{error:#}"))
                    .unwrap_or_default();
                if let Err(error) = agent.store.finish_work(&claim, status, &detail) {
                    tracing::error!(%error, "Could not persist task outcome");
                    // Do not dispatch another job while ownership is uncertain.
                    agent.closing.store(true, Ordering::SeqCst);
                }
                if let Ok(mut active) = agent.cancellation.lock() {
                    active.remove(&claim.task_id);
                }
                agent.emit("state", &claim.task_id, None, None);
                drop(environment);
            }
        });
    }

    pub fn submit(&self, text: &str) -> Result<Task> {
        self.submit_once(text, None)
    }

    pub fn submit_once(&self, text: &str, request_id: Option<&str>) -> Result<Task> {
        let _active = self
            .cancellation
            .lock()
            .map_err(|_| anyhow::anyhow!("Agent state unavailable"))?;
        self.ensure_accepting()?;
        let task = self.store.accept_input(text, request_id)?;
        self.wake.notify_one();
        self.emit("state", &task.id, None, None);
        Ok(task)
    }

    pub fn stop(&self, id: &str) -> Result<()> {
        self.store
            .task(id)?
            .ok_or_else(|| anyhow::anyhow!("Request does not exist"))?;
        let active = self
            .cancellation
            .lock()
            .map_err(|_| anyhow::anyhow!("Agent state unavailable"))?;
        self.store.stop_work(id)?;
        if let Some(token) = active.get(id) {
            token.cancel();
        }
        self.wake.notify_one();
        self.emit("state", id, None, None);
        Ok(())
    }

    // Stop cancels queued work too. Service shutdown below parks it.
    pub fn stop_all(&self) -> Result<()> {
        let active = self
            .cancellation
            .lock()
            .map_err(|_| anyhow::anyhow!("Agent state unavailable"))?;
        let persisted = self.store.stop_all_work();
        for token in active.values() {
            token.cancel();
        }
        self.wake.notify_one();
        persisted
    }

    pub fn begin_shutdown(&self) {
        self.closing.store(true, Ordering::SeqCst);
        self.shutdown.cancel();
        if let Ok(active) = self.cancellation.lock() {
            if let Err(error) = self.store.interrupt_active_work() {
                tracing::error!(%error,"Could not record service interruption");
            }
            for token in active.values() {
                token.cancel();
            }
        }
        self.wake.notify_one();
    }

    pub async fn wait_until_idle(&self) {
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                if self.cancellation.lock().is_ok_and(|map| map.is_empty()) {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await;
    }

    pub async fn shutdown_runtime(&self) -> Result<()> {
        let _environment = self.environment.lock().await;
        if let Some(goose) = &self.goose {
            goose.shutdown().await?;
        }
        Ok(())
    }

    fn ensure_accepting(&self) -> Result<()> {
        anyhow::ensure!(
            !self.closing.load(Ordering::SeqCst),
            "Ayati is stopping; retry after restart"
        );
        Ok(())
    }
}

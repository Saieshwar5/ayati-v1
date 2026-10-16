use super::Agent;
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicBool, Ordering};

struct ClearStopping<'a>(&'a AtomicBool);
impl Drop for ClearStopping<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

impl Agent {
    pub async fn environment_status(&self) -> Value {
        let mut state = match &self.goose {
            Some(goose) => goose.environment_status().await,
            None => json!({"execution":"prototype"}),
        };
        state["active_leases"] = json!(self.leases.active().ok());
        state["dispatch_paused"] = json!(self.store.dispatch_paused().ok());
        state["idle_timeout_seconds"] = json!(self.idle_timeout.map(|d| d.as_secs()));
        state["stopping"] = json!(self.environment_stopping.load(Ordering::SeqCst));
        state["browser_owner"] = json!(self.goose.as_ref().is_some_and(|g| g.browser_owned()));
        state
    }

    pub async fn start_environment(&self) -> Result<Value> {
        let _environment = self.environment.lock().await;
        let lease = self.leases.acquire("owner_start", "local-agent")?;
        self.ensure_accepting()?;
        let goose = self.goose.as_ref().context("VM execution requires Goose")?;
        tokio::select! {
            biased;
            _ = self.shutdown.cancelled() => anyhow::bail!("Ayati is stopping"),
            result = goose.start_vm() => { result?; }
        }
        // Stop may have arrived during boot. Never let its completion unpause it.
        {
            let _active = self
                .cancellation
                .lock()
                .map_err(|_| anyhow::anyhow!("Agent state unavailable"))?;
            self.ensure_accepting()?;
            self.store.resume_dispatch()?;
        }
        self.wake.notify_one();
        self.emit("state", "", None, None);
        drop(lease);
        Ok(self.environment_status().await)
    }

    pub async fn stop_environment(&self) -> Result<Value> {
        let goose = self.goose.as_ref().context("VM execution requires Goose")?;
        ensure!(
            goose.is_vm(),
            "Select VM execution before stopping an agent VM"
        );
        {
            let active = self
                .cancellation
                .lock()
                .map_err(|_| anyhow::anyhow!("Agent state unavailable"))?;
            ensure!(
                !self.environment_stopping.load(Ordering::SeqCst),
                "VM is already stopping"
            );
            self.store.stop_all_work()?;
            self.environment_generation.fetch_add(1, Ordering::SeqCst);
            self.environment_stopping.store(true, Ordering::SeqCst);
            for token in active.values() {
                token.cancel();
            }
            goose.request_vm_stop();
        }
        let clearing = ClearStopping(&self.environment_stopping);
        self.wake.notify_one();
        self.emit("state", "", None, None);
        // The worker holds this gate until its model/tool/approval has unwound.
        // New events may be saved meanwhile, but paused dispatch cannot claim them.
        let _environment = self.environment.lock().await;
        let result = goose.shutdown().await;
        drop(clearing);
        result?;
        self.emit("state", "", None, None);
        Ok(self.environment_status().await)
    }

    pub async fn shutdown_runtime(&self) -> Result<()> {
        let _environment = self.environment.lock().await;
        if let Some(goose) = &self.goose {
            goose.shutdown().await?;
        }
        Ok(())
    }

    pub(super) fn ensure_accepting(&self) -> Result<()> {
        ensure!(
            !self.closing.load(Ordering::SeqCst),
            "Ayati is stopping; retry after restart"
        );
        ensure!(
            !self.environment_stopping.load(Ordering::SeqCst),
            "VM is stopping; retry after shutdown completes"
        );
        Ok(())
    }

    // Called only by the single worker, while holding the lifecycle gate.
    pub(super) async fn release_idle_environment(&self) -> Result<()> {
        let Some(timeout) = self.idle_timeout else {
            return Ok(());
        };
        let Some(goose) = &self.goose else {
            return Ok(());
        };
        if goose.is_vm()
            && !goose.browser_owned()
            && !self.closing.load(Ordering::SeqCst)
            && !self.environment_stopping.load(Ordering::SeqCst)
            && self.leases.idle_for(timeout)
            && (self.store.pending_count()? == 0 || self.store.dispatch_paused()?)
            && goose.environment_status().await["state"] == "ready"
            && goose.browser_idle_safe().await?
        {
            tracing::info!("Releasing idle agent VM");
            // Intake can save work during this await. Dispatch is serialized
            // behind shutdown and wakes the guest again on the next iteration.
            goose.shutdown().await?;
        }
        Ok(())
    }
}

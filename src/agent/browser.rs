use super::Agent;
use anyhow::{Context, Result};
use serde_json::{Value, json};

impl Agent {
    pub async fn browser_status(&self) -> Value {
        match &self.goose {
            Some(goose) => goose.browser_status().await,
            None => json!({"configured":false}),
        }
    }
    pub async fn browser_request(&self, request: Value) -> Result<Value> {
        self.ensure_accepting()?;
        let lease = self.leases.acquire("browser_request", "local-agent")?;
        let goose = self.goose.as_ref().context("Browser requires Goose")?;
        let generation = self
            .environment_generation
            .load(std::sync::atomic::Ordering::SeqCst);
        let result = if request["op"] == "take" {
            goose
                .prepare_browser_take(
                    request["token"]
                        .as_str()
                        .context("Missing browser controller")?,
                )
                .await?;
            // Persist pause/cancel first, then wait for the active worker and any
            // idle shutdown to drain. Stop wins even if it finishes while waiting.
            let _environment = self.environment.lock().await;
            self.ensure_accepting()?;
            anyhow::ensure!(
                generation
                    == self
                        .environment_generation
                        .load(std::sync::atomic::Ordering::SeqCst),
                "VM was stopped while browser takeover waited; take control again deliberately"
            );
            goose.browser_request(request).await
        } else {
            goose.browser_request(request).await
        };
        drop(lease);
        self.wake.notify_one();
        self.emit("state", "", None, None);
        result
    }
}

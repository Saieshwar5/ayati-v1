use super::Agent;
use crate::storage::memory::MemoryScope;
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::sync::atomic::Ordering;

struct Editing<'a>(&'a Agent);
impl Drop for Editing<'_> {
    fn drop(&mut self) {
        self.0.memory_editing.store(false, Ordering::SeqCst);
        self.0.wake.notify_one();
    }
}

impl Agent {
    pub fn memory_enabled(&self) -> bool {
        self.goose.as_ref().is_some_and(|g| g.memory_enabled())
    }
    pub async fn edit_memory(
        &self,
        key: Option<&str>,
        content: Option<&str>,
        replaces: Option<String>,
        forget: Option<&str>,
    ) -> Result<Value> {
        ensure!(
            self.memory_enabled(),
            "Enable AYATI_MEMORY=1 with Goose before using memory"
        );
        self.ensure_accepting()?;
        // A correction/forget must not race an agent retaining stale context.
        // Cancellation drains the prompt before the transaction invalidates sessions.
        let _editing = {
            let active = self
                .cancellation
                .lock()
                .map_err(|_| anyhow::anyhow!("Agent state unavailable"))?;
            ensure!(
                self.memory_editing
                    .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok(),
                "Another memory edit is in progress"
            );
            let editing = Editing(self);
            for cancel in active.values() {
                cancel.cancel();
            }
            editing
        };
        let _environment = self.environment.lock().await;
        self.ensure_accepting()?;
        let scope = MemoryScope::local();
        let value = if let Some(id) = forget {
            self.store.forget_memory(&scope, id)?;
            json!({"ok":true})
        } else {
            json!(self.store.owner_memory_note(
                &scope,
                key.ok_or_else(|| anyhow::anyhow!("Missing memory key"))?,
                content.ok_or_else(|| anyhow::anyhow!("Missing memory content"))?,
                replaces
            )?)
        };
        self.emit("state", "", None, None);
        Ok(value)
    }
}

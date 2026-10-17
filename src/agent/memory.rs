use super::Agent;
use crate::storage::memory::MemoryScope;
use anyhow::{Result, ensure};
use serde_json::{Value, json};

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
        let _editing = self.begin_context_edit()?;
        let _environment = self.environment.lock().await;
        self.ensure_accepting()?;
        ensure!(self.memory_enabled(), "Memory is disabled");
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

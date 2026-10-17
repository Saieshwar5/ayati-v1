use super::Agent;
use anyhow::{Result, ensure};
use std::sync::atomic::Ordering;

pub(super) struct ContextEdit<'a>(&'a Agent);
impl Drop for ContextEdit<'_> {
    fn drop(&mut self) {
        self.0.memory_editing.store(false, Ordering::SeqCst);
        self.0.wake.notify_one();
    }
}
impl Agent {
    pub(super) fn begin_context_edit(&self) -> Result<ContextEdit<'_>> {
        let active = self
            .cancellation
            .lock()
            .map_err(|_| anyhow::anyhow!("Agent state unavailable"))?;
        ensure!(
            self.memory_editing
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok(),
            "Another context edit is in progress"
        );
        let edit = ContextEdit(self);
        for cancel in active.values() {
            cancel.cancel();
        }
        Ok(edit)
    }
    pub async fn invalidate_business_context(&self) -> Result<()> {
        let _edit = self.begin_context_edit()?;
        let _environment = self.environment.lock().await;
        self.store.clear_runtime_session()?;
        if let Some(goose) = &self.goose {
            goose.clear_working_context().await;
        }
        self.emit("state", "", None, None);
        Ok(())
    }
}

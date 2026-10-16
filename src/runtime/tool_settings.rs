use super::GooseRuntime;
use crate::storage::profile::UpdateProfile;
use anyhow::{Context, Result, ensure};

impl GooseRuntime {
    pub fn browser_capable(&self) -> bool {
        self.config.vm_manifest.as_ref().is_some_and(|path| {
            crate::vm::config::VmConfig::load_for(path, &self.config.agent_id)
                .is_ok_and(|vm| vm.browser_version.as_deref() == Some("0.39.1"))
        })
    }
    pub fn tool_defaults(&self) -> (bool, bool) {
        (self.config.memory, self.config.browser)
    }
    pub fn tool_edit_guard(
        &self,
        input: &UpdateProfile,
    ) -> Result<tokio::sync::MutexGuard<'_, ()>> {
        if input.browser == Some(true) {
            ensure!(
                self.browser_capable(),
                "This agent needs a compatible browser VM before browser access can be enabled. Existing files were preserved."
            );
        }
        self.browser
            .as_ref()
            .context("Browser control unavailable")?
            .gate
            .try_lock()
            .context("A browser operation is in progress; finish it before editing settings")
    }
}

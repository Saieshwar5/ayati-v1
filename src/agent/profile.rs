use super::Agent;
use crate::storage::profile::{ProfileEdit, UpdateProfile};
use anyhow::{Context, Result, ensure};

impl Agent {
    pub fn browser_capable(&self) -> bool {
        self.goose.as_ref().is_some_and(|g| g.browser_capable())
    }
    pub fn edit_profile(
        &self,
        input: &UpdateProfile,
        original_role: &str,
        original_model: &str,
    ) -> Result<ProfileEdit> {
        let _environment = self
            .environment
            .try_lock()
            .context("Agent is busy. Finish or stop work and retry once it is idle.")?;
        let _active = self
            .cancellation
            .lock()
            .map_err(|_| anyhow::anyhow!("Agent state unavailable"))?;
        self.ensure_accepting()?;
        let goose = self
            .goose
            .as_ref()
            .context("Profile editing requires the Goose runtime")?;
        let _browser_gate = goose.tool_edit_guard(input)?;
        ensure!(
            !goose.browser_owned(),
            "Return browser control before editing this agent"
        );
        let (memory, browser) = goose.tool_defaults();
        let profile =
            self.store
                .update_profile(input, original_role, original_model, memory, browser)?;
        self.emit("state", "", None, None);
        Ok(profile)
    }
}

//! A fresh, tool-free ACP session may be prepared without submitting model input.
use super::*;

pub(crate) struct ChatTurn<'a> {
    pub agent: &'a str,
    pub instructions: &'a str,
    pub timeout: Duration,
}
impl GooseRuntime {
    fn require_chat_only(&self) -> Result<()> {
        ensure!(
            self.vm.is_none()
                && self.config.platform.is_none()
                && !self.config.memory
                && !self.config.browser
                && !self.config.proof_tool
                && !self.shared.memory_enabled()
                && !self.shared.browser_enabled(),
            "Prepared sessions require the tool-free shared chat profile"
        );
        Ok(())
    }
    pub(crate) async fn prepare_chat(&self) -> Result<u32> {
        self.require_chat_only()?;
        let mut process = self.process.lock().await;
        ensure!(process.is_none(), "Prepare only a fresh chat session");
        let prepared = self.connect().await?;
        let pid = prepared
            .resources
            .child
            .id()
            .context("Prepared Goose process exited")?;
        *process = Some(prepared);
        Ok(pid)
    }
    pub(crate) async fn chat_ready(&self) -> bool {
        let mut process = self.process.lock().await;
        process.as_mut().is_some_and(|p| {
            !p.resources.driver.is_finished() && matches!(p.resources.child.try_wait(), Ok(None))
        })
    }
    pub(crate) async fn prompt_chat(
        &self,
        claim: &RunClaim,
        reply: &str,
        text: String,
        cancel: CancellationToken,
        turn: ChatTurn<'_>,
    ) -> Result<()> {
        self.require_chat_only()?;
        ensure!(
            (1..=86400).contains(&turn.timeout.as_secs()),
            "Invalid prepared execution deadline"
        );
        self.prompt_inner(claim, reply, text, cancel, Some(turn))
            .await
    }
}

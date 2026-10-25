//! A fresh, tool-free ACP session completes generic initialization without inference.
use super::*;

pub(super) fn preparation_ack(
    meta: Option<&serde_json::Map<String, serde_json::Value>>,
) -> Result<u64> {
    let ack = meta.and_then(|meta| meta.get("ayatiPrepared"));
    ensure!(
        ack.is_some_and(|ack| ack["version"] == 1 && ack["tokenCounter"] == "o200k_base"),
        "Prepared Goose did not acknowledge generic preparation v1"
    );
    let duration = ack
        .and_then(|ack| ack["durationMicros"].as_u64())
        .context("Prepared Goose omitted its initialization duration")?;
    ensure!(
        duration <= 30_000_000,
        "Generic preparation exceeded the startup limit"
    );
    Ok(duration)
}

pub(crate) struct ChatTurn<'a> {
    pub assignment: &'a str,
    pub agent: &'a str,
    pub instructions: &'a str,
    pub timeout: Duration,
}
impl GooseRuntime {
    fn require_chat_only(&self) -> Result<()> {
        ensure!(
            self.vm.is_none()
                && self.config.platform.is_none()
                && self.config.control_tools.is_none()
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
        let prepared = self.connect_prepared().await?;
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

#[cfg(test)]
mod tests {
    use super::preparation_ack;
    use serde_json::json;
    #[test]
    fn readiness_requires_a_supported_complete_generic_preparation_ack() {
        let valid =
            json!({"ayatiPrepared":{"version":1,"tokenCounter":"o200k_base","durationMicros":123}});
        assert_eq!(preparation_ack(valid.as_object()).unwrap(), 123);
        assert!(preparation_ack(None).is_err());
        for bad in [
            json!({}),
            json!({"ayatiPrepared":{"version":2,"tokenCounter":"o200k_base","durationMicros":123}}),
            json!({"ayatiPrepared":{"version":1,"tokenCounter":"other","durationMicros":123}}),
            json!({"ayatiPrepared":{"version":1,"tokenCounter":"o200k_base"}}),
            json!({"ayatiPrepared":{"version":1,"tokenCounter":"o200k_base","durationMicros":30_000_001}}),
        ] {
            assert!(preparation_ack(bad.as_object()).is_err());
        }
    }
}

//! One request in a private per-run journal: accept, claim, reply and read back.
use crate::journal::{Journal, work::RunClaim};
use anyhow::{Context, Result, ensure};

/// Record exactly one request, keyed by its controller reply ID, and claim it.
pub(crate) fn begin(store: &Journal, request: &str) -> Result<(RunClaim, String)> {
    let task = store.accept_input("Controller-scoped shared request", Some(request))?;
    let claim = store
        .claim_next(&uuid::Uuid::new_v4().to_string())?
        .context("Run claim unavailable")?;
    ensure!(
        claim.task_id == task.id,
        "A run journal must contain only its assigned request"
    );
    let reply = store.add_claim_reply(&claim)?;
    Ok((claim, reply))
}

/// Close the claim with the prompt outcome and return the completed reply text.
pub(crate) fn finish(
    store: &Journal,
    claim: &RunClaim,
    reply: &str,
    result: Result<()>,
) -> Result<String> {
    let status = if result.is_ok() {
        "completed"
    } else {
        "failed"
    };
    store.finish_work(claim, status, "")?;
    result?;
    Ok(store
        .messages()?
        .into_iter()
        .find(|m| m.id == reply && m.state == "complete")
        .context("The run produced no completed reply")?
        .content)
}

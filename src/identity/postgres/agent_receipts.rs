//! Delivered and seen ticks for messages an agent works on (C2).
use super::*;

/// Code may run against an older schema during an upgrade rehearsal (schema 39 adds the table).
async fn exists(tx: &DatabaseTransaction) -> Result<bool> {
    Ok(query(
        tx,
        "SELECT to_regclass('relay_agent_receipts') IS NOT NULL AS present",
        vec![],
    )
    .await?
    .context("Receipt schema check missing")?
    .try_get("", "present")?)
}

/// The sender's message became work for this agent (grey double tick).
pub(super) async fn delivered(
    tx: &DatabaseTransaction,
    w: &str,
    chat: &str,
    message: &str,
    agent: &str,
) -> Result<()> {
    if !exists(tx).await? {
        return Ok(());
    }
    execute(tx, "INSERT INTO relay_agent_receipts(workspace,chat,message,agent,delivered_at) VALUES($1,$2,$3,$4,$5)
        ON CONFLICT(workspace,message,agent) DO NOTHING",
        vec![w.into(), chat.into(), message.into(), agent.into(), now().into()]).await?;
    Ok(())
}

/// An attempt started reading the request (blue double tick). Retries keep the first time.
pub(super) async fn seen(
    tx: &DatabaseTransaction,
    w: &str,
    chat: &str,
    message: &str,
    agent: &str,
) -> Result<()> {
    if !exists(tx).await? {
        return Ok(());
    }
    execute(tx, "INSERT INTO relay_agent_receipts(workspace,chat,message,agent,delivered_at,seen_at) VALUES($1,$2,$3,$4,$5,$5)
        ON CONFLICT(workspace,message,agent) DO UPDATE SET seen_at=EXCLUDED.seen_at WHERE relay_agent_receipts.seen_at IS NULL",
        vec![w.into(), chat.into(), message.into(), agent.into(), now().into()]).await?;
    Ok(())
}

//! Writing Audit log entries.
use super::*;

pub(super) async fn audit(
    db: &impl ConnectionTrait,
    actor: &str,
    workspace: Option<&str>,
    action: &str,
    target: &str,
) -> Result<()> {
    // No summary column here: upgrade tests write audit entries at schemas before 44.
    execute(
        db,
        "INSERT INTO relay_audit(actor,workspace,action,target,at) VALUES($1,$2,$3,$4,$5)",
        vec![
            actor.into(),
            workspace.into(),
            action.into(),
            target.into(),
            now().into(),
        ],
    )
    .await?;
    Ok(())
}
/// `summary` is the readable sentence shown in the Audit log and Activity.
pub(super) async fn audit_with_summary(
    db: &impl ConnectionTrait,
    actor: &str,
    workspace: Option<&str>,
    action: &str,
    target: &str,
    summary: &str,
) -> Result<()> {
    execute(
        db,
        "INSERT INTO relay_audit(actor,workspace,action,target,at,summary) VALUES($1,$2,$3,$4,$5,$6)",
        vec![
            actor.into(),
            workspace.into(),
            action.into(),
            target.into(),
            now().into(),
            summary.into(),
        ],
    )
    .await?;
    Ok(())
}

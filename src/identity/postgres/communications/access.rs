use super::*;
use crate::identity::chat::model::Job;

pub(in crate::identity::postgres) async fn exists(db: &impl ConnectionTrait) -> Result<bool> {
    Ok(query(
        db,
        "SELECT to_regclass('relay_communication_inbox') IS NOT NULL AS present",
        vec![],
    )
    .await?
    .context("Missing schema check")?
    .try_get("", "present")?)
}
pub(super) async fn owner(tx: &DatabaseTransaction, user: &User, w: &str) -> Result<()> {
    ensure!(
        role(tx, w, &user.id).await? == "Admin",
        "Only the workspace owner can manage simulated communications"
    );
    Ok(())
}
pub(in crate::identity::postgres) async fn allowed(
    db: &impl ConnectionTrait,
    job: &Job,
) -> Result<bool> {
    if !exists(db).await? {
        return Ok(true);
    }
    Ok(query(
        db,
        "SELECT relay_communication_current($1,$2) AS allowed",
        vec![job.workspace.clone().into(), job.chat.clone().into()],
    )
    .await?
    .context("Missing communication scope")?
    .try_get("", "allowed")?)
}
pub(in crate::identity::postgres) async fn ordinary_chat(
    tx: &DatabaseTransaction,
    w: &str,
    chat: &str,
) -> Result<()> {
    if exists(tx).await? {
        ensure!(
            query(
                tx,
                "SELECT chat FROM relay_communication_conversations WHERE workspace=$1 AND chat=$2",
                vec![w.into(), chat.into()]
            )
            .await?
            .is_none(),
            "Use the bound communication endpoint for this conversation"
        );
    }
    Ok(())
}

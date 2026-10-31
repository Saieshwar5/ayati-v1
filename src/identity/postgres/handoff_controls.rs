use super::*;
use crate::identity::chat::model::Job;
use serde_json::json;
pub(super) async fn snapshot(db: &impl ConnectionTrait, job: &Job) -> Result<Value> {
    let rules=json_rows(db,"SELECT to_jsonb(r)||jsonb_build_object('connection_revision',c.revision) AS data FROM relay_control_effective_rules r JOIN relay_control_connections c ON c.workspace=r.workspace AND c.id=r.connection WHERE r.workspace=$1 AND r.agent=$2 ORDER BY r.connection,r.operation",vec![job.workspace.clone().into(),job.agent.clone().into()]).await?;
    let data = json!(rules);
    ensure!(
        serde_json::to_vec(&data)?.len() <= 60000,
        "Sender permission snapshot exceeds supported handoff size"
    );
    Ok(data)
}
pub(super) async fn allowed(
    db: &impl ConnectionTrait,
    job: &Job,
    connection: &str,
    operation: &str,
) -> Result<bool> {
    let records = super::handoff_access::tree(db, job).await?;
    if records.is_empty() {
        return Ok(true);
    }
    let receiver=query(db,"SELECT to_jsonb(r) AS data FROM relay_control_effective_rules r WHERE workspace=$1 AND agent=$2 AND connection=$3 AND operation=$4",vec![job.workspace.clone().into(),job.agent.clone().into(),connection.into(),operation.into()]).await?;
    let Some(receiver) = receiver else {
        return Ok(false);
    };
    let receiver: Value = receiver.try_get("", "data")?;
    let mut child = job.reply.clone();
    for _ in 0..3 {
        let Some(h) = records.iter().find(|h| h["child"] == child) else {
            return Ok(true);
        };
        let Some(saved) = h["controls"].as_array().and_then(|a| {
            a.iter()
                .find(|r| r["connection"] == connection && r["operation"] == operation)
        }) else {
            return Ok(false);
        };
        let current=query(db,"SELECT to_jsonb(r)||jsonb_build_object('connection_revision',c.revision) AS data FROM relay_control_effective_rules r JOIN relay_control_connections c ON c.workspace=r.workspace AND c.id=r.connection WHERE r.workspace=$1 AND r.agent=$2 AND r.connection=$3 AND r.operation=$4",vec![job.workspace.clone().into(),h["sender"].as_str().unwrap().into(),connection.into(),operation.into()]).await?;
        let Some(current) = current else {
            return Ok(false);
        };
        if current.try_get::<Value>("", "data")? != *saved
            || (saved["mode"] == "ask"
                && (receiver["mode"] != "ask" || receiver["approver"] != saved["approver"]))
        {
            return Ok(false);
        }
        if saved["mode"] == "ask" {
            let access = super::approval_authority::require(
                db,
                &job.workspace,
                h["sender"].as_str().unwrap(),
                saved["approver"].as_str().unwrap(),
            )
            .await;
            if access.as_ref().is_err_and(|e| e.is::<sea_orm::DbErr>()) {
                return Err(access.unwrap_err());
            }
            if access.is_err() {
                return Ok(false);
            }
        }
        child = h["parent"].as_str().unwrap().into();
    }
    Ok(child == super::handoff_access::root(db, job).await?)
}
pub(super) async fn ready_for_new_work(db: &impl ConnectionTrait, job: &Job) -> Result<()> {
    if !super::handoff_access::exists(db).await? {
        return Ok(());
    }
    ensure!(query(db,"SELECT h.id FROM relay_agent_handoffs h JOIN relay_chat_jobs j ON j.workspace=h.workspace AND j.reply=h.parent WHERE h.workspace=$1 AND h.parent=$2 AND (h.state='pending' OR h.run_cycle=j.run_cycle)",vec![job.workspace.clone().into(),job.reply.clone().into()]).await?.is_none(),"Checkpoint the accepted helper request and finish this bounded attempt before more work");
    Ok(())
}

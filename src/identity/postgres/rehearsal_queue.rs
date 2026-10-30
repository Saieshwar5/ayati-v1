//! The "sample" executor task purpose: draft-only samples run on pooled
//! executors through `executor_tasks`; this file decides who may still receive
//! a sample and saves its result. Inference is never rerun automatically.
use super::{executor_tasks::Task, *};
use crate::identity::chat::execution::Prepared;

pub(super) const SAMPLE_NOTICE: &str =
    "Draft-only sample — no connected actions were executed.\n\n";

pub(super) async fn invalidate(tx: &DatabaseTransaction, w: &str, id: &str) -> Result<()> {
    execute(tx,"UPDATE relay_agent_rehearsals SET state='invalidated',claim=NULL,lease_until=0,prepared=NULL,output=NULL,detail='Job access or selected knowledge changed. Start a new sample with current access.',finished_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint WHERE workspace=$1 AND id=$2 AND state IN ('queued','running')",vec![w.into(),id.into()]).await?;
    Ok(())
}

/// The sample is still active and its requester can still read every input.
pub(super) async fn allowed(tx: &DatabaseTransaction, task: &Task, user: &User) -> Result<bool> {
    let data = super::rehearsals::record(tx, &task.workspace, &task.id).await?;
    if !matches!(data["state"].as_str(), Some("queued" | "running")) {
        return Ok(false);
    }
    match super::rehearsal_context::readable(tx, user, &task.workspace, &task.agent, &data).await {
        Ok(_) => Ok(true),
        Err(error) if error.is::<sea_orm::DbErr>() => Err(error),
        Err(_) => Ok(false),
    }
}

pub(super) async fn context(db: &impl ConnectionTrait, task: &Task) -> Result<Prepared> {
    let row = query(db,"SELECT prepared FROM relay_agent_rehearsals WHERE workspace=$1 AND id=$2 AND state IN ('queued','running')",
        vec![task.workspace.clone().into(),task.id.clone().into()]).await?.context(super::worker_control::ControlDenied)?;
    let prepared: Prepared = serde_json::from_value(row.try_get("", "prepared")?)?;
    ensure!(
        prepared.version == 1
            && !prepared.control_tools
            && prepared.run_seconds == 45
            && prepared.workspace == task.workspace
            && prepared.agent == task.agent
            && prepared.reply == task.id,
        "Unsupported sample execution context"
    );
    Ok(prepared)
}

pub(super) async fn started(tx: &DatabaseTransaction, task: &Task, claim: &str) -> Result<()> {
    let changed=execute(tx,"UPDATE relay_agent_rehearsals SET state='running',claim=$3,started_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint,detail='Working on the draft-only sample' WHERE workspace=$1 AND id=$2 AND state='queued'",
        vec![task.workspace.clone().into(),task.id.clone().into(),claim.into()]).await?;
    ensure!(changed == 1, "Sample is no longer queued");
    Ok(())
}

pub(super) async fn finished(
    tx: &DatabaseTransaction,
    task: &Task,
    state: &str,
    text: &str,
) -> Result<()> {
    let output = format!("{SAMPLE_NOTICE}{text}");
    let output =
        (state == "complete" && !text.trim().is_empty() && output.len() <= 65536).then_some(output);
    let (state, detail) = if output.is_some() {
        (
            "completed",
            "Draft-only sample saved. No connected actions were executed.",
        )
    } else {
        (
            "failed",
            "The sample did not finish. Check its configuration or try a new sample explicitly.",
        )
    };
    execute(tx,"UPDATE relay_agent_rehearsals SET state=$3,output=$4,detail=$5,claim=NULL,lease_until=0,prepared=NULL,finished_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint WHERE workspace=$1 AND id=$2 AND state='running'",
        vec![task.workspace.clone().into(),task.id.clone().into(),state.into(),output.into(),detail.into()]).await?;
    Ok(())
}

pub(super) async fn interrupted(tx: &DatabaseTransaction, task: &Task) -> Result<()> {
    execute(tx,"UPDATE relay_agent_rehearsals SET state='interrupted',claim=NULL,lease_until=0,prepared=NULL,detail='The sample executor stopped or its lease expired. Start a new sample explicitly.',finished_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint WHERE workspace=$1 AND id=$2 AND state IN ('queued','running')",
        vec![task.workspace.clone().into(),task.id.clone().into()]).await?;
    Ok(())
}

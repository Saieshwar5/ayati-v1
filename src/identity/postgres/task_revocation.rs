//! Revoking source/request access permanently stops saved continuations, including regrant races.
use super::*;
use crate::identity::chat::store::Job;

pub(super) async fn inaccessible(tx: &DatabaseTransaction, w: &str) -> Result<()> {
    let jobs=rows(tx,"SELECT payload FROM relay_chat_jobs j WHERE workspace=$1 AND state<>'finished' AND EXISTS(SELECT 1 FROM relay_task_checkpoints c WHERE c.workspace=j.workspace AND c.reply=j.reply)",vec![w.into()]).await?;
    for row in jobs {
        let job: Job = serde_json::from_value(row.try_get("", "payload")?)?;
        if super::control_actions::authorize(tx, &job, false)
            .await
            .is_ok()
        {
            continue;
        }
        let claim=query(tx,"UPDATE relay_chat_jobs SET state='finished',lease_until=0,finished_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint WHERE workspace=$1 AND reply=$2 RETURNING claim",vec![w.into(),job.reply.clone().into()]).await?.context("Missing saved task")?.try_get::<Option<String>>("","claim")?;
        execute(tx,"UPDATE relay_chat_messages SET state='cancelled',content='Request cancelled because access changed.' WHERE workspace=$1 AND id=$2 AND state='queued'",vec![w.into(),job.reply.clone().into()]).await?;
        execute(tx,"UPDATE relay_control_actions SET state='invalidated',result='\"Task access changed; no pending operation was dispatched.\"'::jsonb WHERE workspace=$1 AND job=$2 AND state='pending'",vec![w.into(),job.reply.into()]).await?;
        if let Some(claim) = claim {
            execute(tx,"UPDATE relay_worker_assignments SET state='finished',outcome='cancelled',finished_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint WHERE id=$1 AND state IN ('launching','running')",vec![claim.clone().into()]).await?;
            execute(tx,"UPDATE relay_assignment_credentials SET context=NULL,expires=0 WHERE assignment=$1",vec![claim.into()]).await?;
        }
    }
    Ok(())
}

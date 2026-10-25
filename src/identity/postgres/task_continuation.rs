//! A task survives its bounded attempts. Saved data reconstructs a fresh Goose run.
use super::*;
use crate::identity::chat::store::Job;
use serde_json::json;

/// The caller holds the workspace transaction, current claim and source authority.
/// All runtime processes must have stopped before publishing this checkpoint.
pub(super) async fn suspend(
    tx: &DatabaseTransaction,
    job: &Job,
    text: &str,
    outcome: &str,
) -> Result<bool> {
    let row=query(tx,"SELECT run_cycle FROM relay_chat_jobs WHERE workspace=$1 AND reply=$2 AND (EXISTS(SELECT 1 FROM relay_control_actions a WHERE a.workspace=$1 AND a.job=$2 AND a.requires_approval AND a.run_cycle=relay_chat_jobs.run_cycle) OR EXISTS(SELECT 1 FROM relay_task_questions q WHERE q.workspace=$1 AND q.reply=$2 AND q.run_cycle=relay_chat_jobs.run_cycle))",vec![job.workspace.clone().into(),job.reply.clone().into()]).await?;
    let Some(row) = row else { return Ok(false) };
    ensure!(
        text.len() <= 65536 && !text.contains('\0'),
        "Checkpoint exceeds supported size"
    );
    let cycle: i32 = row.try_get("", "run_cycle")?;
    let claim = job.claim.as_ref().context("Unclaimed task checkpoint")?;
    execute(tx,"INSERT INTO relay_task_checkpoints(workspace,reply,cycle,claim,text) VALUES($1,$2,$3,$4,$5)",vec![job.workspace.clone().into(),job.reply.clone().into(),cycle.into(),claim.clone().into(),text.into()]).await?;
    execute(
        tx,
        "UPDATE relay_chat_jobs SET state='waiting',lease_until=0 WHERE workspace=$1 AND reply=$2",
        vec![job.workspace.clone().into(), job.reply.clone().into()],
    )
    .await?;
    execute(tx,"UPDATE relay_worker_assignments SET state='finished',outcome=$2,finished_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint WHERE id=$1 AND state='running'",vec![claim.clone().into(),outcome.into()]).await?;
    execute(tx,"UPDATE relay_chat_messages SET content=$3,progress=jsonb_build_object('state','approval','text','') WHERE workspace=$1 AND id=$2 AND state='queued'",vec![job.workspace.clone().into(),job.reply.clone().into(),text.into()]).await?;
    query(
        tx,
        "SELECT relay_resume_waiting_task($1,$2)",
        vec![job.workspace.clone().into(), job.reply.clone().into()],
    )
    .await?;
    Ok(true)
}

impl PgStore {
    pub(crate) async fn task_continuation(&self, job: &Job) -> Result<Option<String>> {
        let tx = self.read_tx(&job.workspace).await?;
        super::control_actions::authorize(&tx, job, true).await?;
        let checkpoint=query(&tx,"SELECT cycle,text FROM relay_task_checkpoints WHERE workspace=$1 AND reply=$2 ORDER BY cycle DESC LIMIT 1",vec![job.workspace.clone().into(),job.reply.clone().into()]).await?;
        let Some(checkpoint) = checkpoint else {
            return Ok(None);
        };
        let actions=json_rows(&tx,&format!("SELECT {} AS data FROM relay_control_actions a WHERE workspace=$1 AND job=$2 ORDER BY created,id",super::control_rules::VIEW),vec![job.workspace.clone().into(),job.reply.clone().into()]).await?;
        ensure!(
            !actions.iter().any(|a| matches!(
                a["state"].as_str(),
                Some("pending" | "executing" | "uncertain")
            )),
            "Task still has unresolved actions"
        );
        let questions=json_rows(&tx,"SELECT jsonb_build_object('id',id,'key',request_key,'revision',revision,'specification',specification,'state',state,'values',answer,'answered_by',answered_by) AS data FROM relay_task_questions WHERE workspace=$1 AND reply=$2 ORDER BY created_ms,id",vec![job.workspace.clone().into(),job.reply.clone().into()]).await?;
        ensure!(
            !questions.iter().any(|q| q["state"] == "pending"),
            "Task still has unanswered questions"
        );
        let data = json!({"version":1,"cycle":checkpoint.try_get::<i32>("","cycle")?,"checkpoint":checkpoint.try_get::<String>("","text")?,"actions":actions,"questions":questions});
        let data = serde_json::to_string(&data)?;
        ensure!(
            data.len() <= 131072,
            "Saved continuation exceeds supported context size"
        );
        tx.commit().await?;
        Ok(Some(data))
    }
}

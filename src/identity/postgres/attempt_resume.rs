//! A lost attempt does not restart its task from scratch. The controller
//! already holds what the attempt did: its streamed text and every tool call
//! and business action with its result, because all tools run through the
//! controller. The next attempt continues from those records (no per-step
//! traffic from executors). An action still executing or uncertain is never
//! retried automatically: the task waits for a person, then resumes.
use super::*;
use crate::identity::chat::model::Job;
use serde_json::json;

/// Attempts per task (and per resumed continuation) before it is reported failed.
pub(super) const MAX_ATTEMPTS: i32 = 5;
/// Interrupted text kept for the next attempt; the newest text is kept.
const OUTPUT_LIMIT: usize = 65536;

/// Text written by lost attempts since the task's latest checkpoint, oldest first.
async fn interrupted_output(
    tx: &DatabaseTransaction,
    w: &str,
    reply: &str,
) -> Result<Option<String>> {
    let row = query(
        tx,
        "SELECT string_agg(attempt.text, E'\\n' ORDER BY attempt.generation) AS text FROM (
        SELECT a.generation, string_agg(o.text,'' ORDER BY o.sequence) AS text
        FROM relay_worker_assignments a JOIN relay_assignment_output o ON o.assignment=a.id
        WHERE a.workspace=$1 AND a.reply=$2 AND a.state='lost'
        AND a.generation>COALESCE((SELECT max(c.generation) FROM relay_task_checkpoints k
          JOIN relay_worker_assignments c ON c.id=k.claim WHERE k.workspace=$1 AND k.reply=$2),0)
        GROUP BY a.generation) attempt",
        vec![w.into(), reply.into()],
    )
    .await?
    .context("Missing interrupted output")?;
    let Some(text) = row
        .try_get::<Option<String>>("", "text")?
        .filter(|t| !t.trim().is_empty())
    else {
        return Ok(None);
    };
    if text.len() <= OUTPUT_LIMIT {
        return Ok(Some(text));
    }
    let mut start = text.len() - OUTPUT_LIMIT;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    Ok(Some(format!("[earlier text omitted]\n{}", &text[start..])))
}

/// Called during lease recovery, after expired attempts are marked lost: tasks
/// with an action still executing or uncertain wait for a person (attention)
/// with what the lost attempt wrote, instead of being retried.
pub(super) async fn park_uncertain(tx: &DatabaseTransaction) -> Result<()> {
    let parked = rows(tx,"SELECT j.workspace,j.reply,j.claim,j.run_cycle FROM relay_chat_jobs j
        WHERE j.dispatch IN ('assigned','pooled') AND j.state='running'
        AND j.lease_until<=floor(extract(epoch FROM clock_timestamp()))::bigint
        AND EXISTS(SELECT 1 FROM relay_control_actions a WHERE a.workspace=j.workspace AND a.job=j.reply AND a.state IN ('executing','uncertain'))
        FOR UPDATE OF j SKIP LOCKED",vec![]).await?;
    for row in parked {
        let (w, reply): (String, String) =
            (row.try_get("", "workspace")?, row.try_get("", "reply")?);
        let claim: String = row.try_get("", "claim")?;
        let text = interrupted_output(tx, &w, &reply)
            .await?
            .unwrap_or_default();
        execute(tx,"INSERT INTO relay_task_checkpoints(workspace,reply,cycle,claim,text) VALUES($1,$2,$3,$4,$5)
            ON CONFLICT(workspace,reply,cycle) DO UPDATE SET claim=EXCLUDED.claim,text=EXCLUDED.text",
            vec![w.clone().into(),reply.clone().into(),row.try_get::<i32>("","run_cycle")?.into(),claim.into(),text.into()]).await?;
        execute(tx,"UPDATE relay_chat_jobs SET state='waiting',lease_until=0 WHERE workspace=$1 AND reply=$2",vec![w.clone().into(),reply.clone().into()]).await?;
        // Shows the task as needing attention; it resumes once the action is reconciled.
        execute(
            tx,
            "SELECT relay_resume_waiting_task($1,$2)",
            vec![w.into(), reply.into()],
        )
        .await?;
    }
    Ok(())
}

impl PgStore {
    /// What earlier, lost attempts of this task already did, for the next attempt.
    pub(crate) async fn interrupted_continuation(&self, job: &Job) -> Result<Option<String>> {
        let tx = self.read_tx(&job.workspace).await?;
        super::control_actions::authorize(&tx, job, true).await?;
        let output = interrupted_output(&tx, &job.workspace, &job.reply).await?;
        let actions=json_rows(&tx,&format!("SELECT {} AS data FROM relay_control_actions a WHERE workspace=$1 AND job=$2 ORDER BY created,id",super::control_rules::VIEW),vec![job.workspace.clone().into(),job.reply.clone().into()]).await?;
        tx.commit().await?;
        let Some(output) = output else {
            return Ok(None);
        };
        Ok(Some(serde_json::to_string(
            &json!({"version":1,"interrupted_output":output,"actions":actions}),
        )?))
    }
}

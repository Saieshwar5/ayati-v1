//! What a person does on a task's page (schema 144, designs/agent-tasks): pause
//! or resume it, or skip its next run. The person it is for, the agent's owners
//! and admins may.
use super::super::*;
use super::changes::summary;
use super::rules::next_due;
use super::runs::{book_next, book_run, now_ms, pause};

impl PgStore {
    /// Pause or resume a task, for the person it's for, the agent's owners and admins.
    pub(crate) async fn task_set_paused(
        &self,
        user: &User,
        w: &str,
        task: &str,
        paused: bool,
    ) -> Result<Value> {
        let details = self.task_details(user, w, task).await?;
        ensure!(
            details["may_close"] == true,
            "Only the person it's for, the agent's owners and admins can pause it"
        );
        let tx = self.workspace_tx(w).await?;
        let open = query(&tx, "SELECT outcome IS NULL AS open FROM relay_tasks WHERE workspace=$1 AND id=$2 FOR UPDATE", vec![w.into(), task.into()])
            .await?.context("Task unavailable")?.try_get::<bool>("", "open")?;
        ensure!(open, "That task is already closed");
        if paused {
            let name = user
                .display_name
                .clone()
                .unwrap_or_else(|| "a person".into());
            pause(&tx, w, task, &format!("Paused by {name}.")).await?;
        } else {
            execute(&tx, "UPDATE relay_tasks SET paused_ms=NULL,paused_reason=NULL WHERE workspace=$1 AND id=$2", vec![w.into(), task.into()]).await?;
            book_next(&tx, w, task, now_ms(&tx).await?).await?;
        }
        audit(
            &tx,
            &user.id,
            Some(w),
            if paused {
                "task_paused"
            } else {
                "task_resumed"
            },
            task,
        )
        .await?;
        let summary = summary(&tx, w, task).await?;
        tx.commit().await?;
        Ok(summary)
    }

    /// Skip the next run of a repeating task; the one after it is booked.
    pub(crate) async fn task_skip_next(&self, user: &User, w: &str, task: &str) -> Result<Value> {
        let details = self.task_details(user, w, task).await?;
        ensure!(
            details["may_close"] == true,
            "Only the person it's for, the agent's owners and admins can skip a run"
        );
        let tx = self.workspace_tx(w).await?;
        super::runs::timing(&tx, w, task).await?;
        let next = query(&tx, "UPDATE relay_task_wakeups SET state='skipped',detail='Skipped by a person.' WHERE workspace=$1 AND task=$2 AND state='pending' AND kind='repeat' RETURNING due_ms",
            vec![w.into(), task.into()]).await?.context("This task has no next run to skip")?.try_get::<i64>("", "due_ms")?;
        let t = super::runs::timing(&tx, w, task).await?;
        if let Some(rule) = &t.rule
            && let Some(due) = next_due(&tx, w, rule, &t.zone, t.anchor_ms, next).await?
            && t.until_ms.is_none_or(|u| due <= u)
        {
            book_run(&tx, w, task, due).await?;
        }
        audit(&tx, &user.id, Some(w), "task_run_skipped", task).await?;
        let summary = summary(&tx, w, task).await?;
        tx.commit().await?;
        Ok(summary)
    }
}

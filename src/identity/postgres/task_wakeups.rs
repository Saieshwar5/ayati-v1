//! Follow-ups: an agent wakes itself to continue a task (people-and-agents-first,
//! Phase 2 T2). `create_task` with `start_in_minutes` or `follow_up` books one; when
//! it is due, the scheduler loop starts a request for the agent in the task's own
//! conversation, for the person it promised, attached to the task. Ordinary intake
//! checks apply: if that person can no longer ask the agent there, the follow-up is
//! blocked and the task needs fixing. Closing a task cancels its follow-ups.
use super::super::*;
use super::{agent_task, line};
use crate::identity::chat::model::{Job, NewMessage};
use serde::Deserialize;
use serde_json::json;

#[cfg(test)]
#[path = "task_wakeups_tests.rs"]
mod tests;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FollowUp {
    pub task: String,
    pub in_minutes: i64,
    pub note: Option<String>,
}

/// Book the task's one pending follow-up `minutes` from now, replacing any other.
pub(super) async fn book(
    tx: &DatabaseTransaction,
    w: &str,
    task: &str,
    minutes: i64,
    note: Option<String>,
) -> Result<i64> {
    ensure!(
        (1..=43_200).contains(&minutes),
        "A follow-up is 1 minute to 30 days away"
    );
    cancel(tx, w, task).await?;
    let row = query(tx, "INSERT INTO relay_task_wakeups(workspace,id,task,due_ms,note) VALUES($1,$2,$3,(extract(epoch FROM clock_timestamp())*1000)::bigint+$4::bigint*60000,$5) RETURNING due_ms",
        vec![w.into(), uuid::Uuid::new_v4().to_string().into(), task.into(), minutes.into(), note.into()]).await?
        .context("Follow-up not saved")?;
    Ok(row.try_get("", "due_ms")?)
}

/// Cancel the task's pending follow-ups (it closed, or a new one replaces them).
pub(super) async fn cancel(tx: &DatabaseTransaction, w: &str, task: &str) -> Result<()> {
    execute(
        tx,
        "UPDATE relay_task_wakeups SET state='cancelled' WHERE workspace=$1 AND task=$2 AND state='pending'",
        vec![w.into(), task.into()],
    )
    .await?;
    Ok(())
}

impl PgStore {
    /// `follow_up`: check this open task again later.
    pub(crate) async fn follow_up(&self, job: &Job, input: &FollowUp) -> Result<Value> {
        let note = input
            .note
            .as_deref()
            .map(|n| line(n, 400, "the note"))
            .transpose()?;
        let tx = self.workspace_tx(&job.workspace).await?;
        super::super::control_actions::authorize(&tx, job, true).await?;
        let task = agent_task(&tx, job, &input.task).await?;
        ensure!(task["outcome"].is_null(), "That task is already closed");
        ensure!(
            task["kind"] == "commitment",
            "Only a task you promised can have a follow-up; create_task first"
        );
        let due = book(&tx, &job.workspace, &input.task, input.in_minutes, note).await?;
        tx.commit().await?;
        Ok(json!({"task": input.task, "due_ms": due, "state": "booked",
            "message": "Follow-up booked. Tell the person when you'll check again; you'll be woken then to continue this task."}))
    }

    /// Start every follow-up that is due. Run by the scheduler loop every second.
    pub(crate) async fn dispatch_task_wakeups(&self) -> Result<usize> {
        // Like schedules, follow-ups run only with pooled executors.
        if !self.pooled {
            return Ok(0);
        }
        let ready = query(
            &self.db,
            "SELECT to_regclass('relay_task_wakeups') IS NOT NULL AS ready",
            vec![],
        )
        .await?;
        if !ready.is_some_and(|r| r.try_get::<bool>("", "ready").unwrap_or(false)) {
            return Ok(0);
        }
        let due = rows(&self.db, "SELECT workspace,id FROM relay_task_wakeups WHERE state='pending' AND due_ms<=(extract(epoch FROM clock_timestamp())*1000)::bigint ORDER BY due_ms LIMIT 20", vec![]).await?;
        let mut started = 0;
        for row in due {
            let w: String = row.try_get("", "workspace")?;
            let id: String = row.try_get("", "id")?;
            if self.wake(&w, &id).await? {
                started += 1;
            }
        }
        Ok(started)
    }

    async fn wake(&self, w: &str, id: &str) -> Result<bool> {
        let tx = self.workspace_tx(w).await?;
        let Some(row) = query(&tx, "SELECT k.note,t.id AS task,t.title,t.done_when,t.agent,t.requester,t.chat,t.outcome,
              EXISTS(SELECT 1 FROM relay_chat_jobs j WHERE j.workspace=t.workspace AND j.task=t.id AND j.state<>'finished') AS busy
            FROM relay_task_wakeups k JOIN relay_tasks t ON t.workspace=k.workspace AND t.id=k.task
            WHERE k.workspace=$1 AND k.id=$2 AND k.state='pending' FOR UPDATE OF k SKIP LOCKED",
            vec![w.into(), id.into()]).await? else { return Ok(false) };
        let later = |detail: &str| {
            (
                "UPDATE relay_task_wakeups SET due_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint+300000,detail=$3 WHERE workspace=$1 AND id=$2",
                detail.to_string(),
            )
        };
        if row.try_get::<Option<String>>("", "outcome")?.is_some() {
            execute(
                &tx,
                "UPDATE relay_task_wakeups SET state='cancelled' WHERE workspace=$1 AND id=$2",
                vec![w.into(), id.into()],
            )
            .await?;
            tx.commit().await?;
            return Ok(false);
        }
        // Work on the task is still running: check again in five minutes.
        if row.try_get::<bool>("", "busy")? {
            let (sql, detail) = later("Waiting for the task's current work to finish.");
            execute(&tx, sql, vec![w.into(), id.into(), detail.into()]).await?;
            tx.commit().await?;
            return Ok(false);
        }
        let task: String = row.try_get("", "task")?;
        let title: String = row.try_get("", "title")?;
        let done_when: Option<String> = row.try_get("", "done_when")?;
        let note: Option<String> = row.try_get("", "note")?;
        let person = User {
            id: row.try_get("", "requester")?,
            email: String::new(),
            display_name: None,
        };
        let chat: String = row.try_get("", "chat")?;
        let message = uuid::Uuid::new_v4().to_string();
        let input = NewMessage {
            reply_to: None,
            id: message.clone(),
            content: format!(
                "Follow-up · {title}\nYou booked this follow-up for your task \"{title}\" (task {task}){}.{} Check where things stand, do what is due now, then tell the person in one or two sentences. Call close_task when the outcome is reached, or follow_up to check again later.",
                done_when
                    .map(|d| format!("; done when: {d}"))
                    .unwrap_or_default(),
                note.map(|n| format!(" Your note: {n}")).unwrap_or_default(),
            ),
            agent: Some(row.try_get("", "agent")?),
            thread: None,
            attachments: vec![],
        };
        let intake = tx.begin().await?;
        match self.background_in(&intake, &person, w, &chat, &input).await {
            Ok(Some(job)) => {
                intake.commit().await?;
                execute(
                    &tx,
                    "UPDATE relay_chat_jobs SET task=$3 WHERE workspace=$1 AND reply=$2",
                    vec![w.into(), job.reply.clone().into(), task.into()],
                )
                .await?;
                execute(&tx, "UPDATE relay_task_wakeups SET state='started',reply=$3,message=$4,detail='' WHERE workspace=$1 AND id=$2", vec![w.into(), id.into(), job.reply.into(), message.into()]).await?;
                tx.commit().await?;
                Ok(true)
            }
            Ok(None) => {
                intake.rollback().await?;
                execute(&tx, "UPDATE relay_task_wakeups SET state='blocked',detail='The agent did not take the follow-up.' WHERE workspace=$1 AND id=$2", vec![w.into(), id.into()]).await?;
                tx.commit().await?;
                Ok(false)
            }
            Err(error) => {
                intake.rollback().await?;
                if error.is::<sea_orm::DbErr>() {
                    return Err(error);
                }
                let busy = error.is::<crate::identity::chat::model::AgentsPaused>()
                    || error.is::<crate::identity::chat::model::QueueFull>()
                    || error.is::<crate::identity::chat::model::BackgroundQueueFull>();
                if busy {
                    let (sql, detail) =
                        later("Agents are paused or busy; trying again in five minutes.");
                    execute(&tx, sql, vec![w.into(), id.into(), detail.into()]).await?;
                } else {
                    let detail: String = format!("Follow-up blocked: {error}")
                        .chars()
                        .take(300)
                        .collect();
                    execute(&tx, "UPDATE relay_task_wakeups SET state='blocked',detail=$3 WHERE workspace=$1 AND id=$2", vec![w.into(), id.into(), detail.into()]).await?;
                }
                tx.commit().await?;
                Ok(false)
            }
        }
    }
}

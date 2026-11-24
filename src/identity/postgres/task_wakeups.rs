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
    book_kind(tx, w, task, minutes, note, "follow_up").await
}

/// Book a wake-up of `kind` (follow_up or, from schema 144, continue) `minutes`
/// from now, replacing the task's other pending one of that kind.
pub(super) async fn book_kind(
    tx: &DatabaseTransaction,
    w: &str,
    task: &str,
    minutes: i64,
    note: Option<String>,
    kind: &str,
) -> Result<i64> {
    let due = query(
        tx,
        "SELECT (extract(epoch FROM clock_timestamp())*1000)::bigint+$1::bigint*60000 AS due",
        vec![minutes.into()],
    )
    .await?
    .context("Clock unavailable")?
    .try_get("", "due")?;
    book_at(tx, w, task, due, note, kind).await?;
    Ok(due)
}

/// Book a wake-up of `kind` at `due_ms`, replacing the task's other pending one of
/// that kind. Before schema 144 every wake-up is a follow-up.
pub(super) async fn book_at(
    tx: &DatabaseTransaction,
    w: &str,
    task: &str,
    due_ms: i64,
    note: Option<String>,
    kind: &str,
) -> Result<()> {
    let id = uuid::Uuid::new_v4().to_string();
    if !lifetimes(tx).await? {
        cancel(tx, w, task).await?;
        execute(
            tx,
            "INSERT INTO relay_task_wakeups(workspace,id,task,due_ms,note) VALUES($1,$2,$3,$4,$5)",
            vec![w.into(), id.into(), task.into(), due_ms.into(), note.into()],
        )
        .await?;
        return Ok(());
    }
    execute(
        tx,
        "UPDATE relay_task_wakeups SET state='cancelled' WHERE workspace=$1 AND task=$2 AND state='pending' AND kind=$3",
        vec![w.into(), task.into(), kind.into()],
    )
    .await?;
    execute(tx, "INSERT INTO relay_task_wakeups(workspace,id,task,due_ms,note,kind) VALUES($1,$2,$3,$4,$5,$6)",
        vec![w.into(), id.into(), task.into(), due_ms.into(), note.into(), kind.into()]).await?;
    Ok(())
}

/// Cancel the task's pending wake-ups (it closed or paused).
pub(super) async fn cancel(tx: &DatabaseTransaction, w: &str, task: &str) -> Result<()> {
    execute(
        tx,
        "UPDATE relay_task_wakeups SET state='cancelled' WHERE workspace=$1 AND task=$2 AND state='pending'",
        vec![w.into(), task.into()],
    )
    .await?;
    Ok(())
}

/// Tasks with a lifetime (repeat rules, notes, run kinds) exist from schema 144.
pub(crate) async fn lifetimes(db: &impl ConnectionTrait) -> Result<bool> {
    Ok(query(
        db,
        "SELECT to_regclass('relay_task_notes') IS NOT NULL AS ready",
        vec![],
    )
    .await?
    .context("Schema check unavailable")?
    .try_get("", "ready")?)
}

/// Try wake-up `id` again in five minutes, saying why.
pub(super) async fn later(tx: &DatabaseTransaction, w: &str, id: &str, detail: &str) -> Result<()> {
    execute(
        tx,
        "UPDATE relay_task_wakeups SET due_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint+300000,detail=$3 WHERE workspace=$1 AND id=$2",
        vec![w.into(), id.into(), detail.into()],
    )
    .await?;
    Ok(())
}

/// Task rooms, instructions and checks in code exist from schema 145.
pub(crate) async fn rooms(db: &impl ConnectionTrait) -> Result<bool> {
    Ok(query(
        db,
        "SELECT to_regprocedure('relay_task_calendar_due(jsonb,text,bigint,bigint,text)') IS NOT NULL AS ready",
        vec![],
    )
    .await?
    .context("Schema check unavailable")?
    .try_get("", "ready")?)
}

/// The person a task is for and where its runs happen: its work room when a
/// channel asked for it (schema 145), otherwise its own conversation.
pub(super) async fn owner(
    tx: &DatabaseTransaction,
    w: &str,
    task: &str,
) -> Result<Option<(User, String)>> {
    let place = if rooms(tx).await? {
        "COALESCE(room,chat)"
    } else {
        "chat"
    };
    let Some(row) = query(
        tx,
        &format!("SELECT requester,{place} AS chat FROM relay_tasks WHERE workspace=$1 AND id=$2"),
        vec![w.into(), task.into()],
    )
    .await?
    else {
        return Ok(None);
    };
    let person = User {
        id: row.try_get("", "requester")?,
        email: String::new(),
        display_name: None,
    };
    Ok(Some((person, row.try_get("", "chat")?)))
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

    /// Coming up: the person it's for, the agent's owner and admins stop a
    /// booked follow-up. The task stays open.
    pub(crate) async fn task_cancel_follow_up(
        &self,
        user: &User,
        w: &str,
        task: &str,
    ) -> Result<()> {
        let details = self.task_details(user, w, task).await?;
        ensure!(
            details["may_close"] == true,
            "Only the person it's for, the agent's owner and admins can stop its follow-up"
        );
        let tx = self.workspace_tx(w).await?;
        cancel(&tx, w, task).await?;
        audit(&tx, &user.id, Some(w), "task_follow_up_cancelled", task).await?;
        tx.commit().await?;
        Ok(())
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
        let kinds = if lifetimes(&tx).await? {
            "k.kind,k.due_ms"
        } else {
            "'follow_up' AS kind,k.due_ms"
        };
        let Some(row) = query(&tx, &format!("SELECT {kinds},k.note,t.id AS task,t.title,t.done_when,t.agent,t.outcome,
              EXISTS(SELECT 1 FROM relay_chat_jobs j WHERE j.workspace=t.workspace AND j.task=t.id AND j.state<>'finished') AS busy
            FROM relay_task_wakeups k JOIN relay_tasks t ON t.workspace=k.workspace AND t.id=k.task
            WHERE k.workspace=$1 AND k.id=$2 AND k.state='pending' FOR UPDATE OF k SKIP LOCKED"),
            vec![w.into(), id.into()]).await? else { return Ok(false) };
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
        let kind: String = row.try_get("", "kind")?;
        if kind == "repeat" || kind == "end" {
            return self.wake_run(tx, w, id, &row).await;
        }
        // Work on the task is still running: check again in five minutes.
        if row.try_get::<bool>("", "busy")? {
            later(&tx, w, id, "Waiting for the task's current work to finish.").await?;
            tx.commit().await?;
            return Ok(false);
        }
        let task: String = row.try_get("", "task")?;
        let title: String = row.try_get("", "title")?;
        let done_when: Option<String> = row.try_get("", "done_when")?;
        let note: Option<String> = row.try_get("", "note")?;
        let Some((person, chat)) = owner(&tx, w, &task).await? else {
            return Ok(false);
        };
        let done_when = done_when
            .map(|d| format!("; done when: {d}"))
            .unwrap_or_default();
        let content = if kind == "continue" {
            format!(
                "Continue · {title}\nYou are continuing your long task \"{title}\" (task {task}){done_when}, part by part.{}{} Do the next part now. When the whole task is finished, call close_task. If more is left, call continue_later again with what is done and what is next.",
                note.map(|n| format!(" Where you stopped: {n}."))
                    .unwrap_or_default(),
                super::runs::notes_text(&tx, w, &task).await?,
            )
        } else {
            format!(
                "Follow-up · {title}\nYou booked this follow-up for your task \"{title}\" (task {task}){done_when}.{} Check where things stand, do what is due now, then tell the person in one or two sentences. Call close_task when the outcome is reached, or follow_up to check again later.",
                note.map(|n| format!(" Your note: {n}")).unwrap_or_default(),
            )
        };
        let input = NewMessage {
            reply_to: None,
            id: uuid::Uuid::new_v4().to_string(),
            content,
            agent: Some(row.try_get("", "agent")?),
            agents: vec![],
            thread: None,
            attachments: vec![],
        };
        let started = self
            .start_wakeup(&tx, w, id, &person, &chat, &input, &task)
            .await?;
        tx.commit().await?;
        Ok(started)
    }

    /// Start the request of wake-up `id` for `person` in `chat`, attached to `task`.
    /// When the agent can't take it now (paused, busy) it is tried again in five
    /// minutes; when the person can no longer ask it there, the wake-up is blocked.
    /// Does not commit `tx`.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn start_wakeup(
        &self,
        tx: &DatabaseTransaction,
        w: &str,
        id: &str,
        person: &User,
        chat: &str,
        input: &NewMessage,
        task: &str,
    ) -> Result<bool> {
        let intake = tx.begin().await?;
        match self.background_in(&intake, person, w, chat, input).await {
            Ok(Some(job)) => {
                intake.commit().await?;
                execute(
                    tx,
                    "UPDATE relay_chat_jobs SET task=$3 WHERE workspace=$1 AND reply=$2",
                    vec![w.into(), job.reply.clone().into(), task.into()],
                )
                .await?;
                execute(tx, "UPDATE relay_task_wakeups SET state='started',reply=$3,message=$4,detail='' WHERE workspace=$1 AND id=$2", vec![w.into(), id.into(), job.reply.into(), input.id.clone().into()]).await?;
                Ok(true)
            }
            Ok(None) => {
                intake.rollback().await?;
                execute(tx, "UPDATE relay_task_wakeups SET state='blocked',detail='The agent did not take the follow-up.' WHERE workspace=$1 AND id=$2", vec![w.into(), id.into()]).await?;
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
                    later(
                        tx,
                        w,
                        id,
                        "Agents are paused or busy; trying again in five minutes.",
                    )
                    .await?;
                } else {
                    let detail: String = format!("Follow-up blocked: {error}")
                        .chars()
                        .take(300)
                        .collect();
                    execute(tx, "UPDATE relay_task_wakeups SET state='blocked',detail=$3 WHERE workspace=$1 AND id=$2", vec![w.into(), id.into(), detail.into()]).await?;
                }
                Ok(false)
            }
        }
    }
}

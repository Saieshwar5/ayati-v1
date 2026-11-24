//! Runs of a task with a lifetime (schema 144, designs/agent-tasks). Cuckoo, not
//! the agent, books every run of a repeating task: when a run starts, the next one
//! is booked from the calendar rule in the same transaction. A run that is missed
//! or due while the last one is still going is skipped and recorded; three failed
//! runs in a row pause the task. When the rule, end date or run count leaves no
//! next run, an end check closes the task once its last run has finished.
use super::super::*;
use super::rules::{Rule, next_due};
use crate::identity::chat::model::{Job, NewMessage};
use sea_orm::QueryResult;

/// A task's timing, read for update in the transaction that books its next run.
pub(super) struct Timing {
    pub rule: Option<Rule>,
    pub zone: String,
    pub anchor_ms: i64,
    pub until_ms: Option<i64>,
    pub max_runs: Option<i32>,
    pub runs: i32,
    pub open: bool,
    pub paused: bool,
}

pub(super) async fn timing(tx: &DatabaseTransaction, w: &str, task: &str) -> Result<Timing> {
    let row = query(
        tx,
        "SELECT t.repeat,COALESCE(t.timezone,ws.timezone,'Asia/Kolkata') AS zone,COALESCE(t.anchor_ms,t.created_ms) AS anchor_ms,
          t.until_ms,t.max_runs,t.runs,t.outcome IS NULL AS open,t.paused_ms IS NOT NULL AS paused
         FROM relay_tasks t JOIN relay_workspaces ws ON ws.id=t.workspace WHERE t.workspace=$1 AND t.id=$2 FOR UPDATE OF t",
        vec![w.into(), task.into()],
    )
    .await?
    .context("Task unavailable")?;
    let rule: Option<Value> = row.try_get("", "repeat")?;
    Ok(Timing {
        rule: rule.map(serde_json::from_value).transpose()?,
        zone: row.try_get("", "zone")?,
        anchor_ms: row.try_get("", "anchor_ms")?,
        until_ms: row.try_get("", "until_ms")?,
        max_runs: row.try_get("", "max_runs")?,
        runs: row.try_get("", "runs")?,
        open: row.try_get("", "open")?,
        paused: row.try_get("", "paused")?,
    })
}

pub(crate) async fn now_ms(db: &impl ConnectionTrait) -> Result<i64> {
    Ok(query(
        db,
        "SELECT (extract(epoch FROM clock_timestamp())*1000)::bigint AS now",
        vec![],
    )
    .await?
    .context("Clock unavailable")?
    .try_get("", "now")?)
}

/// Book run `due_ms` of the task, or make a cancelled booking of it pending again.
pub(super) async fn book_run(
    tx: &DatabaseTransaction,
    w: &str,
    task: &str,
    due: i64,
) -> Result<()> {
    execute(
        tx,
        "INSERT INTO relay_task_wakeups(workspace,id,task,due_ms,kind) VALUES($1,$2,$3,$4,'repeat')
         ON CONFLICT (workspace,task,due_ms) WHERE kind='repeat' DO UPDATE SET state='pending',detail=''
         WHERE relay_task_wakeups.state='cancelled'",
        vec![
            w.into(),
            uuid::Uuid::new_v4().to_string().into(),
            task.into(),
            due.into(),
        ],
    )
    .await?;
    Ok(())
}

/// Cancel the task's pending runs and end check (a change rebooks them).
pub(super) async fn cancel_runs(tx: &DatabaseTransaction, w: &str, task: &str) -> Result<()> {
    execute(
        tx,
        "UPDATE relay_task_wakeups SET state='cancelled' WHERE workspace=$1 AND task=$2 AND state='pending' AND kind IN ('repeat','end')",
        vec![w.into(), task.into()],
    )
    .await?;
    Ok(())
}

/// Book the next run of a repeating task after `after_ms`, replacing pending ones.
/// When the rule, end date or run count leaves none, book the end check instead.
pub(super) async fn book_next(
    tx: &DatabaseTransaction,
    w: &str,
    task: &str,
    after_ms: i64,
) -> Result<Option<i64>> {
    cancel_runs(tx, w, task).await?;
    let t = timing(tx, w, task).await?;
    let Some(rule) = t.rule.as_ref().filter(|_| t.open && !t.paused) else {
        return Ok(None);
    };
    let left = t.max_runs.is_none_or(|max| t.runs < max);
    let next = if left {
        next_due(tx, w, rule, &t.zone, t.anchor_ms, after_ms).await?
    } else {
        None
    };
    match next.filter(|due| t.until_ms.is_none_or(|until| *due <= until)) {
        Some(due) => {
            book_run(tx, w, task, due).await?;
            Ok(Some(due))
        }
        None => {
            execute(
                tx,
                "INSERT INTO relay_task_wakeups(workspace,id,task,due_ms,kind) VALUES($1,$2,$3,(extract(epoch FROM clock_timestamp())*1000)::bigint,'end')",
                vec![w.into(), uuid::Uuid::new_v4().to_string().into(), task.into()],
            )
            .await?;
            Ok(None)
        }
    }
}

async fn mark(
    tx: &DatabaseTransaction,
    w: &str,
    id: &str,
    state: &str,
    detail: &str,
) -> Result<()> {
    execute(
        tx,
        "UPDATE relay_task_wakeups SET state=$3,detail=$4 WHERE workspace=$1 AND id=$2",
        vec![w.into(), id.into(), state.into(), detail.into()],
    )
    .await?;
    Ok(())
}

/// Pause the task and stop its pending runs; it shows as paused in Needs you.
pub(super) async fn pause(
    tx: &DatabaseTransaction,
    w: &str,
    task: &str,
    reason: &str,
) -> Result<()> {
    execute(
        tx,
        "UPDATE relay_tasks SET paused_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint,paused_reason=$3 WHERE workspace=$1 AND id=$2 AND outcome IS NULL",
        vec![w.into(), task.into(), reason.into()],
    )
    .await?;
    execute(
        tx,
        "UPDATE relay_task_wakeups SET state='cancelled' WHERE workspace=$1 AND task=$2 AND state='pending'",
        vec![w.into(), task.into()],
    )
    .await?;
    Ok(())
}

/// How many of the task's last three runs failed.
async fn failed_in_a_row(tx: &DatabaseTransaction, w: &str, task: &str) -> Result<i64> {
    Ok(query(
        tx,
        "SELECT count(*) FILTER (WHERE m.state IN ('failed','interrupted','access_changed')) AS failed,count(*) AS runs
         FROM (SELECT k.reply FROM relay_task_wakeups k WHERE k.workspace=$1 AND k.task=$2 AND k.state='started'
               AND k.kind IN ('repeat','continue') AND k.reply IS NOT NULL ORDER BY k.due_ms DESC LIMIT 3) r
         JOIN relay_chat_messages m ON m.workspace=$1 AND m.id=r.reply",
        vec![w.into(), task.into()],
    )
    .await?
    .map(|r| -> Result<i64> {
        let (failed, runs): (i64, i64) = (r.try_get("", "failed")?, r.try_get("", "runs")?);
        Ok(if runs == 3 { failed } else { 0 })
    })
    .transpose()?
    .unwrap_or(0))
}

/// The last notes runs left, newest first, as lines for the next run's brief.
pub(super) async fn notes_text(tx: &DatabaseTransaction, w: &str, task: &str) -> Result<String> {
    let notes = rows(
        tx,
        "SELECT to_char(to_timestamp(n.created_ms/1000.0) AT TIME ZONE COALESCE(t.timezone,ws.timezone,'Asia/Kolkata'),'DD Mon') AS day,n.note
         FROM relay_task_notes n JOIN relay_tasks t ON t.workspace=n.workspace AND t.id=n.task JOIN relay_workspaces ws ON ws.id=n.workspace
         WHERE n.workspace=$1 AND n.task=$2 ORDER BY n.created_ms DESC LIMIT 8",
        vec![w.into(), task.into()],
    )
    .await?;
    let mut text = String::new();
    for row in notes {
        let day: String = row.try_get("", "day")?;
        let note: String = row.try_get("", "note")?;
        text.push_str(&format!("\n- {day}: {note}"));
    }
    Ok(if text.is_empty() {
        "\nNo notes yet: this is the first run.".into()
    } else {
        format!("\nNotes from earlier runs, newest first:{text}")
    })
}

/// Close a task whose rule, end date or run count has run out, once its last run
/// has finished: done for a duty that simply ended, gave up for a task waiting
/// for something that did not happen.
async fn close_ran_out(tx: &DatabaseTransaction, w: &str, task: &str) -> Result<()> {
    execute(
        tx,
        "UPDATE relay_tasks SET outcome=CASE WHEN done_when IS NULL THEN 'done' ELSE 'failed' END,
          result=CASE WHEN done_when IS NULL THEN format('Finished after %s runs.',runs)
                 ELSE format('Gave up after %s runs: %s',runs,done_when) END,
          closed_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint
         WHERE workspace=$1 AND id=$2 AND outcome IS NULL",
        vec![w.into(), task.into()],
    )
    .await?;
    Ok(())
}

impl PgStore {
    /// Start a repeat run or an end check (`kind` repeat or end) whose wake-up row
    /// `row` is locked in `tx`. Commits `tx`.
    pub(super) async fn wake_run(
        &self,
        tx: DatabaseTransaction,
        w: &str,
        id: &str,
        row: &QueryResult,
    ) -> Result<bool> {
        let task: String = row.try_get("", "task")?;
        let kind: String = row.try_get("", "kind")?;
        let busy: bool = row.try_get("", "busy")?;
        if kind == "end" {
            if busy {
                super::wakeups::later(&tx, w, id, "Waiting for the last run to finish.").await?;
            } else {
                close_ran_out(&tx, w, &task).await?;
                mark(&tx, w, id, "started", "The task ended.").await?;
            }
            tx.commit().await?;
            return Ok(false);
        }
        let t = timing(&tx, w, &task).await?;
        let Some(rule) = t.rule.clone().filter(|_| t.open && !t.paused) else {
            mark(&tx, w, id, "cancelled", "").await?;
            tx.commit().await?;
            return Ok(false);
        };
        let now = now_ms(&tx).await?;
        let due: i64 = row.try_get("", "due_ms")?;
        let grace = (rule.period_ms() / 2).clamp(600_000, 43_200_000);
        let skip = if now - due > grace {
            Some("Missed: Cuckoo was not running at that time.")
        } else if busy {
            Some("Skipped: the last run was still going.")
        } else {
            None
        };
        if let Some(detail) = skip {
            mark(&tx, w, id, "skipped", detail).await?;
            book_next(&tx, w, &task, now).await?;
            tx.commit().await?;
            return Ok(false);
        }
        if failed_in_a_row(&tx, w, &task).await? >= 3 {
            pause(
                &tx,
                w,
                &task,
                "Cuckoo paused it after 3 failed runs in a row.",
            )
            .await?;
            mark(
                &tx,
                w,
                id,
                "skipped",
                "Paused after 3 failed runs in a row.",
            )
            .await?;
            tx.commit().await?;
            return Ok(false);
        }
        let run = t.runs + 1;
        let input = self
            .run_request(&tx, w, &task, &rule, run, t.max_runs)
            .await?;
        let Some((person, chat)) = super::wakeups::owner(&tx, w, &task).await? else {
            mark(&tx, w, id, "cancelled", "").await?;
            tx.commit().await?;
            return Ok(false);
        };
        let started = self
            .start_wakeup(&tx, w, id, &person, &chat, &input, &task)
            .await?;
        if started {
            execute(
                &tx,
                "UPDATE relay_task_wakeups SET run=$3 WHERE workspace=$1 AND id=$2",
                vec![w.into(), id.into(), run.into()],
            )
            .await?;
            execute(
                &tx,
                "UPDATE relay_tasks SET runs=runs+1 WHERE workspace=$1 AND id=$2",
                vec![w.into(), task.clone().into()],
            )
            .await?;
            book_next(&tx, w, &task, now).await?;
        }
        tx.commit().await?;
        Ok(started)
    }

    /// The request a run starts with: a short brief written by Cuckoo.
    async fn run_request(
        &self,
        tx: &DatabaseTransaction,
        w: &str,
        task: &str,
        rule: &Rule,
        run: i32,
        max_runs: Option<i32>,
    ) -> Result<NewMessage> {
        let row = query(
            tx,
            "SELECT t.title,t.done_when,t.agent,t.until_ms,
              to_char(to_timestamp(t.until_ms/1000.0) AT TIME ZONE COALESCE(t.timezone,ws.timezone,'Asia/Kolkata'),'DD Mon YYYY') AS until
             FROM relay_tasks t JOIN relay_workspaces ws ON ws.id=t.workspace WHERE t.workspace=$1 AND t.id=$2",
            vec![w.into(), task.into()],
        )
        .await?
        .context("Task unavailable")?;
        let title: String = row.try_get("", "title")?;
        let done_when: Option<String> = row.try_get("", "done_when")?;
        let until: Option<String> = row.try_get("", "until")?;
        let of = max_runs.map(|m| format!(" of {m}")).unwrap_or_default();
        let last = max_runs.is_some_and(|m| run >= m);
        let mut brief = format!(
            "Task run {run}{of} · {title}\nCuckoo started run {run}{of} of your task \"{title}\" (task {task}) on its rule: {}{}.",
            rule.sentence(),
            until.map(|u| format!(", until {u}")).unwrap_or_default()
        );
        if let Some(done_when) = &done_when {
            brief.push_str(&format!(
                " It is finished when: {done_when}. Check that first; if it is true, tell the person and call close_task with done."
            ));
        }
        if last {
            brief.push_str(if done_when.is_some() {
                " This is the last run: if it is still not finished, tell the person it is over to them."
            } else {
                " This is the last run."
            });
        }
        if super::wakeups::rooms(tx).await?
            && let Some(instructions) = query(
                tx,
                "SELECT instructions FROM relay_tasks WHERE workspace=$1 AND id=$2",
                vec![w.into(), task.into()],
            )
            .await?
            .and_then(|r| {
                r.try_get::<Option<String>>("", "instructions")
                    .ok()
                    .flatten()
            })
        {
            brief.push_str(&format!("\nWhat to do each run: {instructions}"));
        }
        brief.push_str(&notes_text(tx, w, task).await?);
        brief.push_str("\nDo what is due now. Then call task_note with one line for the next run. If nothing changed and there is nothing to tell the person, reply with exactly NO_CHANGE and nothing else.");
        Ok(NewMessage {
            reply_to: None,
            id: uuid::Uuid::new_v4().to_string(),
            content: brief,
            agent: Some(row.try_get("", "agent")?),
            agents: vec![],
            thread: None,
            attachments: vec![],
        })
    }
}

/// After a task run finished: a run that answered NO_CHANGE is kept out of the
/// conversation and recorded as "no change".
pub(crate) async fn after_finish(tx: &DatabaseTransaction, job: &Job, content: &str) -> Result<()> {
    if !super::wakeups::lifetimes(tx).await? {
        return Ok(());
    }
    if content.lines().next().map(str::trim) == Some("NO_CHANGE") {
        let quiet = query(
            tx,
            "UPDATE relay_task_wakeups SET quiet=TRUE WHERE workspace=$1 AND reply=$2 AND kind='repeat' RETURNING message",
            vec![job.workspace.clone().into(), job.reply.clone().into()],
        )
        .await?;
        let Some(row) = quiet else { return Ok(()) };
        let message: Option<String> = row.try_get("", "message")?;
        execute(
            tx,
            "UPDATE relay_chat_messages SET content='',attachments='[]',state='deleted' WHERE workspace=$1 AND id IN ($2,$3)",
            vec![
                job.workspace.clone().into(),
                job.reply.clone().into(),
                message.unwrap_or_default().into(),
            ],
        )
        .await?;
        return Ok(());
    }
    // A task a channel asked for works in its room and posts each run's result to
    // the channel (schema 145, the owner's channel rule).
    if super::wakeups::rooms(tx).await?
        && let Some(row) = query(
            tx,
            "SELECT t.title,t.requester,t.chat FROM relay_task_wakeups k JOIN relay_tasks t ON t.workspace=k.workspace AND t.id=k.task
             WHERE k.workspace=$1 AND k.reply=$2 AND t.room=$3 AND t.chat<>t.room",
            vec![job.workspace.clone().into(), job.reply.clone().into(), job.chat.clone().into()],
        )
        .await?
    {
        let (title, requester, chat): (String, String, String) =
            (row.try_get("", "title")?, row.try_get("", "requester")?, row.try_get("", "chat")?);
        super::super::work_rooms::post(tx, job, content, &title, &requester, &chat, None).await?;
    }
    Ok(())
}

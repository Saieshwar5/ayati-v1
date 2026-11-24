//! Timing and changes of a task with a lifetime (schema 144, designs/agent-tasks).
//! People say it in chat and the agent does it: `create_task` with a rule starts a
//! repeating task at once, `change_task` changes or pauses it, `task_note` leaves a
//! line for the next run and `continue_later` splits a long job into parts. The
//! person a task is for, the agent's owners and admins pause, resume or skip a run.
use super::super::*;
use super::rules::{Rule, next_due};
use super::runs::{book_next, book_run, cancel_runs, now_ms, pause};
use super::time::{end_of_day_ms, local_ms};
pub(crate) use super::time::{shown, workspace_zone};
use super::{agent_task, line};
use crate::identity::chat::model::Job;
use serde::Deserialize;
use serde_json::json;

/// Repeating tasks one person may have with one agent, and an agent in all.
const PER_PERSON: i64 = 20;
const PER_AGENT: i64 = 100;
/// Parts of one long job before the agent must ask the person.
const MAX_PIECES: i32 = 20;

/// When a task runs, as the agent gives it.
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Timing {
    pub repeat: Option<Rule>,
    /// Local date and time of the first run, "2026-11-26 10:00".
    pub first_at: Option<String>,
    /// Local date of the last day it may run, "2026-12-12".
    pub until: Option<String>,
    pub max_runs: Option<i64>,
}

impl Timing {
    pub(crate) fn any(&self) -> bool {
        self.repeat.is_some()
            || self.first_at.is_some()
            || self.until.is_some()
            || self.max_runs.is_some()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ChangeTask {
    pub task: String,
    pub title: Option<String>,
    pub done_when: Option<String>,
    pub repeat: Option<Rule>,
    pub first_at: Option<String>,
    pub until: Option<String>,
    pub max_runs: Option<i64>,
    pub paused: Option<bool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TaskNote {
    pub task: String,
    pub note: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ContinueLater {
    pub task: String,
    pub done: String,
    pub next: String,
}

/// Save `timing` on `task` of `job`'s agent and book its first run; returns the
/// first run. A one-off time without a rule is a follow-up at that time. A run that
/// started on its own (schedule, event, task run) may only create a repeating task
/// that ends.
pub(super) async fn schedule(
    tx: &DatabaseTransaction,
    job: &Job,
    task: &str,
    timing: &Timing,
    done_when: Option<&str>,
) -> Result<Option<i64>> {
    let w = job.workspace.as_str();
    if let Some(rule) = &timing.repeat {
        rule.validate()?;
    }
    ensure!(
        timing.max_runs.is_none_or(|n| (1..=1000).contains(&n)),
        "max_runs is 1 to 1000"
    );
    let zone = query(tx, "SELECT COALESCE(t.timezone,ws.timezone,'Asia/Kolkata') AS zone FROM relay_tasks t JOIN relay_workspaces ws ON ws.id=t.workspace WHERE t.workspace=$1 AND t.id=$2",
        vec![w.into(), task.into()]).await?.context("Task unavailable")?.try_get::<String>("", "zone")?;
    let now = now_ms(tx).await?;
    let first = match &timing.first_at {
        Some(text) => {
            let ms = local_ms(tx, text, &zone).await?;
            ensure!(ms > now, "first_at must be in the future");
            Some(ms)
        }
        None => None,
    };
    let until = match &timing.until {
        Some(text) => Some(end_of_day_ms(tx, text, &zone).await?),
        None => None,
    };
    ensure!(
        until.is_none_or(|u| u > now && first.is_none_or(|f| f <= u)),
        "until must be after now and after the first run"
    );
    if timing.repeat.is_some() {
        let background = query(tx, "SELECT work_class='background' AS background FROM relay_chat_jobs WHERE workspace=$1 AND reply=$2", vec![w.into(), job.reply.clone().into()])
            .await?.map(|r| r.try_get::<bool>("", "background")).transpose()?.unwrap_or(false);
        ensure!(
            !background || until.is_some() || timing.max_runs.is_some() || done_when.is_some(),
            "A run that started on its own can only create a repeating task that ends: give until, max_runs or done_when"
        );
        let counts = query(
            tx,
            "SELECT count(*) FILTER (WHERE requester=$3) AS mine,count(*) AS agent FROM relay_tasks
            WHERE workspace=$1 AND agent=$2 AND repeat IS NOT NULL AND outcome IS NULL AND id<>$4",
            vec![
                w.into(),
                job.agent.clone().into(),
                job.requester.clone().into(),
                task.into(),
            ],
        )
        .await?
        .context("Count unavailable")?;
        ensure!(
            counts.try_get::<i64>("", "mine")? < PER_PERSON,
            "This person already has {PER_PERSON} repeating tasks with you; end one first"
        );
        ensure!(
            counts.try_get::<i64>("", "agent")? < PER_AGENT,
            "You already have {PER_AGENT} repeating tasks; end one first"
        );
    }
    let anchor = first.unwrap_or(now);
    execute(
        tx,
        "UPDATE relay_tasks SET repeat=$3,anchor_ms=CASE WHEN $3::jsonb IS NULL THEN anchor_ms ELSE $4 END,
          until_ms=COALESCE($5,until_ms),max_runs=COALESCE($6,max_runs),
          start_ms=COALESCE($7,start_ms) WHERE workspace=$1 AND id=$2",
        vec![
            w.into(),
            task.into(),
            timing.repeat.as_ref().map(serde_json::to_value).transpose()?.into(),
            anchor.into(),
            until.into(),
            timing.max_runs.map(|n| n as i32).into(),
            first.into(),
        ],
    )
    .await?;
    match (&timing.repeat, first) {
        (Some(_), Some(first)) => {
            cancel_runs(tx, w, task).await?;
            book_run(tx, w, task, first).await?;
            Ok(Some(first))
        }
        (Some(_), None) => book_next(tx, w, task, now).await,
        (None, Some(first)) => {
            super::wakeups::book_at(tx, w, task, first, None, "follow_up").await?;
            Ok(Some(first))
        }
        (None, None) => Ok(None),
    }
}

/// The task as one sentence and its next run, for the agent's reply and the screens.
pub(crate) async fn summary(db: &impl ConnectionTrait, w: &str, task: &str) -> Result<Value> {
    let Some(row) = query(db, "SELECT t.repeat,t.max_runs,t.runs,t.done_when,t.paused_ms IS NOT NULL AS paused,
          COALESCE(t.timezone,ws.timezone,'Asia/Kolkata') AS zone,
          to_char(to_timestamp(t.until_ms/1000.0) AT TIME ZONE COALESCE(t.timezone,ws.timezone,'Asia/Kolkata'),'DD Mon YYYY') AS until,
          (SELECT min(k.due_ms) FROM relay_task_wakeups k WHERE k.workspace=t.workspace AND k.task=t.id AND k.state='pending' AND k.kind<>'end') AS next_ms
         FROM relay_tasks t JOIN relay_workspaces ws ON ws.id=t.workspace WHERE t.workspace=$1 AND t.id=$2",
        vec![w.into(), task.into()]).await? else { return Ok(Value::Null) };
    let rule: Option<Value> = row.try_get("", "repeat")?;
    let rule: Option<Rule> = rule.map(serde_json::from_value).transpose()?;
    let zone: String = row.try_get("", "zone")?;
    let next: Option<i64> = row.try_get("", "next_ms")?;
    let until: Option<String> = row.try_get("", "until")?;
    let max_runs: Option<i32> = row.try_get("", "max_runs")?;
    let done_when: Option<String> = row.try_get("", "done_when")?;
    let mut sentence = rule.as_ref().map(Rule::sentence).unwrap_or_default();
    if let Some(done_when) = &done_when
        && rule.is_some()
    {
        sentence.push_str(&format!(", until {done_when}"));
    }
    if let Some(until) = &until {
        sentence.push_str(&format!(", until {until}"));
    }
    if let Some(max) = max_runs {
        let word = if done_when.is_some() {
            "gives up after"
        } else {
            "at most"
        };
        sentence.push_str(&format!(", {word} {max} runs"));
    }
    let next_shown = match next {
        Some(ms) => Some(shown(db, ms, &zone).await?),
        None => None,
    };
    Ok(
        json!({"rule": rule, "rule_text": (!sentence.is_empty()).then_some(sentence),
        "timezone": zone, "next_ms": next, "next_text": next_shown, "runs": row.try_get::<i32>("", "runs")?,
        "max_runs": max_runs, "paused": row.try_get::<bool>("", "paused")?}),
    )
}

impl PgStore {
    /// `change_task`: change, pause or resume one of the agent's open tasks.
    pub(crate) async fn change_task(&self, job: &Job, input: &ChangeTask) -> Result<Value> {
        let title = input
            .title
            .as_deref()
            .map(|t| line(t, 160, "the title"))
            .transpose()?;
        let done_when = input
            .done_when
            .as_deref()
            .map(|d| line(d, 400, "done_when"))
            .transpose()?;
        let timing = Timing {
            repeat: input.repeat.clone(),
            first_at: input.first_at.clone(),
            until: input.until.clone(),
            max_runs: input.max_runs,
        };
        ensure!(
            title.is_some() || done_when.is_some() || timing.any() || input.paused.is_some(),
            "Say what to change"
        );
        let tx = self.workspace_tx(&job.workspace).await?;
        super::super::control_actions::authorize(&tx, job, true).await?;
        let task = agent_task(&tx, job, &input.task).await?;
        ensure!(task["outcome"].is_null(), "That task is already closed");
        let w = job.workspace.as_str();
        execute(&tx, "UPDATE relay_tasks SET title=COALESCE($3,title),done_when=COALESCE($4,done_when),kind='commitment' WHERE workspace=$1 AND id=$2",
            vec![w.into(), input.task.clone().into(), title.into(), done_when.clone().into()]).await?;
        if input.paused == Some(true) {
            pause(&tx, w, &input.task, "Paused in chat.").await?;
        } else {
            if input.paused == Some(false) {
                execute(&tx, "UPDATE relay_tasks SET paused_ms=NULL,paused_reason=NULL WHERE workspace=$1 AND id=$2", vec![w.into(), input.task.clone().into()]).await?;
            }
            if timing.any() {
                let rule_kept = if timing.repeat.is_none() {
                    query(
                        &tx,
                        "SELECT repeat FROM relay_tasks WHERE workspace=$1 AND id=$2",
                        vec![w.into(), input.task.clone().into()],
                    )
                    .await?
                    .and_then(|r| r.try_get::<Option<Value>>("", "repeat").ok().flatten())
                    .map(serde_json::from_value)
                    .transpose()?
                } else {
                    None
                };
                let timing = Timing {
                    repeat: timing.repeat.or(rule_kept),
                    ..timing
                };
                schedule(&tx, job, &input.task, &timing, done_when.as_deref()).await?;
            } else if input.paused == Some(false) {
                book_next(&tx, w, &input.task, now_ms(&tx).await?).await?;
            }
        }
        let summary = summary(&tx, w, &input.task).await?;
        tx.commit().await?;
        Ok(
            json!({"id": input.task, "state": if input.paused == Some(true) { "paused" } else { "changed" }, "task": summary,
            "message": "Saved. Tell the person in one line what changed and when it runs next."}),
        )
    }

    /// `task_note`: one line the task's next run reads.
    pub(crate) async fn task_note(&self, job: &Job, input: &TaskNote) -> Result<Value> {
        let note = line(&input.note, 400, "the note")?;
        let tx = self.workspace_tx(&job.workspace).await?;
        super::super::control_actions::authorize(&tx, job, true).await?;
        agent_task(&tx, job, &input.task).await?;
        add_note(&tx, &job.workspace, &input.task, &note, Some(&job.reply)).await?;
        tx.commit().await?;
        Ok(json!({"task": input.task, "state": "saved"}))
    }

    /// `continue_later`: save where this part of a long job stopped; Cuckoo starts
    /// the next part in a minute. After 20 parts the agent must ask the person.
    pub(crate) async fn continue_later(&self, job: &Job, input: &ContinueLater) -> Result<Value> {
        let done = line(&input.done, 400, "done")?;
        let next = line(&input.next, 400, "next")?;
        let tx = self.workspace_tx(&job.workspace).await?;
        super::super::control_actions::authorize(&tx, job, true).await?;
        let task = agent_task(&tx, job, &input.task).await?;
        ensure!(task["outcome"].is_null(), "That task is already closed");
        ensure!(
            task["kind"] == "commitment",
            "Only a task you promised can continue later; create_task first"
        );
        let w = job.workspace.as_str();
        let pieces: i32 = query(
            &tx,
            "UPDATE relay_tasks SET pieces=pieces+1 WHERE workspace=$1 AND id=$2 RETURNING pieces",
            vec![w.into(), input.task.clone().into()],
        )
        .await?
        .context("Task unavailable")?
        .try_get("", "pieces")?;
        if pieces > MAX_PIECES {
            return Ok(json!({"task": input.task, "state": "refused",
                "message": format!("This task has already run in {MAX_PIECES} parts. Tell the person what is done and ask whether to go on.")}));
        }
        add_note(
            &tx,
            w,
            &input.task,
            &format!("Part {pieces} done: {done}"),
            Some(&job.reply),
        )
        .await?;
        let due = super::wakeups::book_kind(
            &tx,
            w,
            &input.task,
            1,
            Some(line(&format!("Done: {done}. Next: {next}"), 400, "the note").unwrap_or(next)),
            "continue",
        )
        .await?;
        tx.commit().await?;
        Ok(
            json!({"task": input.task, "state": "continuing", "part": pieces + 1, "due_ms": due,
            "message": "Saved. Reply in one line with what is done so far; the next part starts in about a minute."}),
        )
    }

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

/// Save one note on a task, keeping its newest 20.
pub(super) async fn add_note(
    tx: &DatabaseTransaction,
    w: &str,
    task: &str,
    note: &str,
    reply: Option<&str>,
) -> Result<()> {
    execute(
        tx,
        "INSERT INTO relay_task_notes(workspace,id,task,note,reply) VALUES($1,$2,$3,$4,$5)",
        vec![
            w.into(),
            uuid::Uuid::new_v4().to_string().into(),
            task.into(),
            note.into(),
            reply.map(str::to_string).into(),
        ],
    )
    .await?;
    execute(tx, "DELETE FROM relay_task_notes WHERE workspace=$1 AND task=$2 AND id NOT IN (SELECT id FROM relay_task_notes WHERE workspace=$1 AND task=$2 ORDER BY created_ms DESC LIMIT 20)",
        vec![w.into(), task.into()]).await?;
    Ok(())
}

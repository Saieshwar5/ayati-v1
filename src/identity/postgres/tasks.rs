//! Tasks (people-and-agents-first, Phase 2): a task is a promise with an outcome.
//!
//! The platform opens an automatic task when a request changes a real system,
//! waits for someone, or starts from an event; it closes with that
//! request. An agent opens a commitment with `create_task` when it promises
//! something that outlives its reply, attaches later work with `attach_to_task`
//! and closes it with `close_task`. A person can turn a request into a task.
//! Status is derived from the task's requests (`relay_task_status`), never stored.
use super::*;
use crate::identity::chat::model::Job;
use serde::Deserialize;
use serde_json::json;

#[cfg(test)]
#[path = "task_change_tests.rs"]
mod change_tests;
#[path = "task_changes.rs"]
pub(crate) mod changes;
#[cfg(test)]
#[path = "schedules_to_tasks_tests.rs"]
mod conversion_tests;
#[cfg(test)]
#[path = "task_lifetimes_tests.rs"]
pub(in crate::identity::postgres) mod lifetime_tests;
#[path = "tasks_migration.rs"]
pub(super) mod migration;
#[path = "task_rules.rs"]
pub(crate) mod rules;
#[path = "task_runs.rs"]
pub(crate) mod runs;
#[cfg(test)]
#[path = "tasks_tests.rs"]
mod tests;
#[path = "task_time.rs"]
pub(crate) mod time;
#[path = "task_views.rs"]
pub(super) mod views;
#[path = "task_wakeups.rs"]
pub(super) mod wakeups;

/// Who may see task `t` ($2 the person, $3 admin): the person it is for, anyone who
/// can see its conversation, the agent's owner and admins.
pub(super) const VISIBLE: &str = "(t.requester=$2 OR $3
 OR relay_agent_owns(t.workspace,t.agent,$2)
 OR EXISTS(SELECT 1 FROM relay_chats c WHERE c.workspace=t.workspace AND c.id=t.chat
  AND (c.visibility='public' OR EXISTS(SELECT 1 FROM relay_chat_people p WHERE p.workspace=c.workspace AND p.chat=c.id AND p.user_id=$2))))";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateTask {
    pub title: String,
    pub done_when: Option<String>,
    /// Start later: the agent is woken to continue the task then (1 minute to 30 days).
    pub start_in_minutes: Option<i64>,
    /// Due by: the task shows as late after this (1 minute to a year).
    pub due_in_minutes: Option<i64>,
    /// Schema 144: run it on a calendar rule, first at a local time, until a local
    /// date or for a number of runs. Cuckoo books every run.
    pub repeat: Option<rules::Rule>,
    pub first_at: Option<String>,
    pub until: Option<String>,
    pub max_runs: Option<i64>,
    /// Schema 145: what each run must do, when the title is not enough.
    pub instructions: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AttachTask {
    pub task: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CloseTask {
    pub task: String,
    pub outcome: String,
    pub result: Option<String>,
}

/// Text trimmed to one line of at most `max` characters, or an error naming `what`.
pub(super) fn line(text: &str, max: usize, what: &str) -> Result<String> {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    ensure!(!text.is_empty(), "{what} can't be empty");
    ensure!(
        text.chars().count() <= max,
        "Keep {what} to {max} characters"
    );
    Ok(text)
}

async fn job_task(tx: &DatabaseTransaction, job: &Job) -> Result<Option<(String, String)>> {
    let row = query(
        tx,
        "SELECT t.id,t.kind FROM relay_chat_jobs j JOIN relay_tasks t ON t.workspace=j.workspace AND t.id=j.task
         WHERE j.workspace=$1 AND j.reply=$2 FOR UPDATE OF j",
        vec![job.workspace.clone().into(), job.reply.clone().into()],
    )
    .await?;
    row.map(|r| Ok((r.try_get("", "id")?, r.try_get("", "kind")?)))
        .transpose()
}

/// Tasks exist from schema 58. Upgrade tests run current code on older schemas,
/// where requests simply aren't tracked as tasks.
pub(super) async fn available(db: &impl ConnectionTrait) -> Result<bool> {
    Ok(query(
        db,
        "SELECT to_regclass('relay_tasks') IS NOT NULL AS ready",
        vec![],
    )
    .await?
    .context("Schema check unavailable")?
    .try_get("", "ready")?)
}

/// Open an automatic task for `job` unless it already works towards one. Called in
/// the same transaction as the write, question, hand-off or event
/// that makes it a task. A Home panel refresh is a panel update, never a task.
pub(super) async fn ensure_automatic(
    tx: &DatabaseTransaction,
    job: &Job,
    origin: &str,
) -> Result<()> {
    if !available(tx).await? {
        return Ok(());
    }
    let row = query(
        tx,
        "SELECT j.task,EXISTS(SELECT 1 FROM relay_home_update_runs hu WHERE hu.workspace=j.workspace AND hu.reply=j.reply) AS panel,
          COALESCE(
           (SELECT t.source_definition->>'name' FROM relay_event_deliveries d JOIN relay_event_triggers t ON t.workspace=d.workspace AND t.id=d.trigger_id
             WHERE d.workspace=j.workspace AND d.reply=j.reply),
           (SELECT m.content FROM relay_chat_messages m WHERE m.workspace=j.workspace AND m.id=j.request_message),
           'Request') AS title
         FROM relay_chat_jobs j WHERE j.workspace=$1 AND j.reply=$2 FOR UPDATE",
        vec![job.workspace.clone().into(), job.reply.clone().into()],
    )
    .await?
    .context("Request unavailable")?;
    if row.try_get::<Option<String>>("", "task")?.is_some() || row.try_get::<bool>("", "panel")? {
        return Ok(());
    }
    let raw: String = row.try_get("", "title")?;
    let mut title: String = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if title.chars().count() > 160 {
        title = title.chars().take(159).collect::<String>() + "…";
    }
    if title.is_empty() {
        title = "Request".into();
    }
    let id = uuid::Uuid::new_v4().to_string();
    execute(tx, "INSERT INTO relay_tasks(workspace,id,agent,kind,origin,title,requester,chat) VALUES($1,$2,$3,'automatic',$4,$5,$6,$7)",
        vec![job.workspace.clone().into(), id.clone().into(), job.agent.clone().into(), origin.into(), title.into(), job.requester.clone().into(), job.chat.clone().into()]).await?;
    execute(
        tx,
        "UPDATE relay_chat_jobs SET task=$3 WHERE workspace=$1 AND reply=$2",
        vec![
            job.workspace.clone().into(),
            job.reply.clone().into(),
            id.into(),
        ],
    )
    .await?;
    Ok(())
}

/// A repeating task Cuckoo makes for `person` when they turn something on (an
/// agent's weekly review): it works in its own new work room and shows only in
/// Activity. Its first run is booked at `first_ms`. Returns the task's id.
#[allow(clippy::too_many_arguments)]
pub(super) async fn create_repeating(
    tx: &DatabaseTransaction,
    w: &str,
    agent: &str,
    person: &str,
    title: &str,
    instructions: &str,
    rule: &rules::Rule,
    first_ms: i64,
) -> Result<String> {
    let room = super::work_rooms::open(tx, w, title, "Runs of a task", person, agent).await?;
    let id = uuid::Uuid::new_v4().to_string();
    execute(tx, "INSERT INTO relay_tasks(workspace,id,agent,kind,origin,title,requester,chat,created_by,instructions,repeat,anchor_ms)
        VALUES($1,$2,$3,'commitment','person',$4,$5,$6,$5,$7,$8,$9)",
        vec![w.into(), id.clone().into(), agent.into(), title.into(), person.into(), room.into(), instructions.into(),
            serde_json::to_value(rule)?.into(), first_ms.into()]).await?;
    runs::book_run(tx, w, &id, first_ms).await?;
    Ok(id)
}

/// A task the agent of `job` may work on: its own, for the same person or in the
/// same conversation.
pub(super) async fn agent_task(tx: &DatabaseTransaction, job: &Job, id: &str) -> Result<Value> {
    let row = query(tx, "SELECT jsonb_build_object('id',t.id,'kind',t.kind,'title',t.title,'outcome',t.outcome,'status',relay_task_status(t.workspace,t.id)) AS data
        FROM relay_tasks t WHERE t.workspace=$1 AND t.id=$2 AND t.agent=$3 AND (t.requester=$4 OR t.chat=$5) FOR UPDATE",
        vec![job.workspace.clone().into(), id.into(), job.agent.clone().into(), job.requester.clone().into(), job.chat.clone().into()]).await?
        .context("Task unavailable to this request")?;
    Ok(row.try_get("", "data")?)
}

impl PgStore {
    /// `create_task`: the agent promises something that outlives this reply.
    pub(crate) async fn create_task(&self, job: &Job, input: &CreateTask) -> Result<Value> {
        let title = line(&input.title, 160, "the title")?;
        ensure!(
            input
                .start_in_minutes
                .is_none_or(|m| (1..=43_200).contains(&m)),
            "start_in_minutes is 1 to 43200 (30 days)"
        );
        ensure!(
            input
                .due_in_minutes
                .is_none_or(|m| (1..=525_600).contains(&m)),
            "due_in_minutes is 1 to 525600 (a year)"
        );
        let done_when = input
            .done_when
            .as_deref()
            .map(|d| line(d, 400, "done_when"))
            .transpose()?;
        let tx = self.workspace_tx(&job.workspace).await?;
        super::control_actions::authorize(&tx, job, true).await?;
        let id = match job_task(&tx, job).await? {
            Some((id, kind)) if kind == "commitment" => {
                let current: String = query(
                    &tx,
                    "SELECT title FROM relay_tasks WHERE workspace=$1 AND id=$2",
                    vec![job.workspace.clone().into(), id.clone().into()],
                )
                .await?
                .unwrap()
                .try_get("", "title")?;
                ensure!(
                    current == title,
                    "This request already works towards the task \"{current}\". Use close_task when it is done"
                );
                id
            }
            // The platform already opened a task for this request: it becomes the promise.
            Some((id, _)) => {
                execute(&tx, "UPDATE relay_tasks SET kind='commitment',origin='agent',title=$3,done_when=$4 WHERE workspace=$1 AND id=$2",
                    vec![job.workspace.clone().into(), id.clone().into(), title.clone().into(), done_when.clone().into()]).await?;
                id
            }
            None => {
                let id = uuid::Uuid::new_v4().to_string();
                execute(&tx, "INSERT INTO relay_tasks(workspace,id,agent,kind,origin,title,done_when,requester,chat) VALUES($1,$2,$3,'commitment','agent',$4,$5,$6,$7)",
                    vec![job.workspace.clone().into(), id.clone().into(), job.agent.clone().into(), title.clone().into(), done_when.clone().into(), job.requester.clone().into(), job.chat.clone().into()]).await?;
                execute(
                    &tx,
                    "UPDATE relay_chat_jobs SET task=$3 WHERE workspace=$1 AND reply=$2",
                    vec![
                        job.workspace.clone().into(),
                        job.reply.clone().into(),
                        id.clone().into(),
                    ],
                )
                .await?;
                id
            }
        };
        execute(&tx, "UPDATE relay_tasks SET start_ms=CASE WHEN $3::bigint IS NULL THEN start_ms ELSE (extract(epoch FROM clock_timestamp())*1000)::bigint+$3::bigint*60000 END,
            due_ms=CASE WHEN $4::bigint IS NULL THEN due_ms ELSE (extract(epoch FROM clock_timestamp())*1000)::bigint+$4::bigint*60000 END WHERE workspace=$1 AND id=$2",
            vec![job.workspace.clone().into(), id.clone().into(), input.start_in_minutes.into(), input.due_in_minutes.into()]).await?;
        if let Some(minutes) = input.start_in_minutes {
            wakeups::book(&tx, &job.workspace, &id, minutes, None).await?;
        }
        let timing = changes::Timing {
            repeat: input.repeat.clone(),
            first_at: input.first_at.clone(),
            until: input.until.clone(),
            max_runs: input.max_runs,
        };
        if let Some(text) = &input.instructions {
            let text = changes::instructions_text(text)?;
            changes::set_instructions(&tx, &job.workspace, &id, &text).await?;
        }
        if timing.any() {
            ensure!(
                wakeups::lifetimes(&tx).await?,
                "Repeating tasks need a newer workspace schema"
            );
            ensure!(
                input.start_in_minutes.is_none(),
                "Give either start_in_minutes or first_at, not both"
            );
            changes::schedule(&tx, job, &id, &timing, done_when.as_deref()).await?;
        }
        let summary = if wakeups::lifetimes(&tx).await? {
            changes::summary(&tx, &job.workspace, &id).await?
        } else {
            Value::Null
        };
        tx.commit().await?;
        let message = if input.repeat.is_some() {
            "Task saved and running: Cuckoo starts every run on its rule, in this conversation. Tell the person in one or two sentences what you will do, when it runs next (next_text) and what ends it. Don't ask them to confirm."
        } else if input.start_in_minutes.is_some() || input.first_at.is_some() {
            "Task saved. You'll be woken to continue it at its start time; tell the person when that is."
        } else {
            "Task saved. Keep working on it now if you can; when its outcome is reached, call close_task. To check again later, call follow_up."
        };
        Ok(json!({"id": id, "title": title, "state": "open", "task": summary, "message": message}))
    }

    /// `attach_to_task`: this request continues an open task.
    pub(crate) async fn attach_to_task(&self, job: &Job, input: &AttachTask) -> Result<Value> {
        let tx = self.workspace_tx(&job.workspace).await?;
        super::control_actions::authorize(&tx, job, true).await?;
        let task = agent_task(&tx, job, &input.task).await?;
        ensure!(task["outcome"].is_null(), "That task is already closed");
        match job_task(&tx, job).await? {
            Some((id, _)) if id == input.task => {}
            Some((id, kind)) => {
                ensure!(
                    kind == "automatic",
                    "This request already works towards another task"
                );
                execute(
                    &tx,
                    "UPDATE relay_chat_jobs SET task=$3 WHERE workspace=$1 AND reply=$2",
                    vec![
                        job.workspace.clone().into(),
                        job.reply.clone().into(),
                        input.task.clone().into(),
                    ],
                )
                .await?;
                // The platform's automatic task for this request is no longer needed.
                execute(&tx, "DELETE FROM relay_tasks t WHERE t.workspace=$1 AND t.id=$2 AND t.kind='automatic' AND NOT EXISTS(SELECT 1 FROM relay_chat_jobs j WHERE j.workspace=t.workspace AND j.task=t.id)",
                    vec![job.workspace.clone().into(), id.into()]).await?;
            }
            None => {
                execute(
                    &tx,
                    "UPDATE relay_chat_jobs SET task=$3 WHERE workspace=$1 AND reply=$2",
                    vec![
                        job.workspace.clone().into(),
                        job.reply.clone().into(),
                        input.task.clone().into(),
                    ],
                )
                .await?;
            }
        }
        tx.commit().await?;
        Ok(json!({"id": task["id"], "title": task["title"], "state": "attached"}))
    }

    /// `open_tasks`: this agent's open tasks for this person or conversation, so it
    /// attaches to one instead of promising the same thing twice.
    pub(crate) async fn open_tasks(&self, job: &Job) -> Result<Value> {
        let tx = self.read_tx(&job.workspace).await?;
        let items = json_rows(&tx, "SELECT jsonb_build_object('id',t.id,'title',t.title,'done_when',t.done_when,'status',relay_task_status(t.workspace,t.id),
            'due_ms',t.due_ms,'start_ms',t.start_ms,'here',t.chat=$4) AS data
            FROM relay_tasks t WHERE t.workspace=$1 AND t.agent=$2 AND t.kind='commitment' AND t.outcome IS NULL AND (t.requester=$3 OR t.chat=$4)
            ORDER BY t.created_ms DESC LIMIT 20",
            vec![job.workspace.clone().into(), job.agent.clone().into(), job.requester.clone().into(), job.chat.clone().into()]).await?;
        // The agent is not told the time otherwise; it needs it to give first_at and until.
        let clock = if wakeups::lifetimes(&tx).await? {
            let zone = changes::workspace_zone(&tx, &job.workspace).await?;
            let now = query(
                &tx,
                "SELECT to_char(clock_timestamp() AT TIME ZONE $1,'Dy YYYY-MM-DD HH24:MI') AS now",
                vec![zone.clone().into()],
            )
            .await?
            .context("Clock unavailable")?
            .try_get::<String>("", "now")?;
            json!({"now": now, "timezone": zone})
        } else {
            json!({})
        };
        tx.commit().await?;
        let mut result = json!({"tasks": items});
        if let (Some(result), Some(clock)) = (result.as_object_mut(), clock.as_object()) {
            result.extend(clock.clone());
        }
        Ok(result)
    }

    /// `close_task`: the agent reached (or gave up on) a task's outcome.
    pub(crate) async fn close_task(&self, job: &Job, input: &CloseTask) -> Result<Value> {
        ensure!(
            ["done", "failed", "cancelled"].contains(&input.outcome.as_str()),
            "outcome is done, failed or cancelled"
        );
        let result = input
            .result
            .as_deref()
            .map(|r| line(r, 1000, "the result"))
            .transpose()?;
        let tx = self.workspace_tx(&job.workspace).await?;
        super::control_actions::authorize(&tx, job, true).await?;
        let task = agent_task(&tx, job, &input.task).await?;
        if task["outcome"] == input.outcome.as_str() {
            tx.commit().await?;
            return Ok(json!({"id": task["id"], "state": input.outcome}));
        }
        ensure!(task["outcome"].is_null(), "That task is already closed");
        execute(&tx, "UPDATE relay_tasks SET outcome=$3,result=$4,closed_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint WHERE workspace=$1 AND id=$2",
            vec![job.workspace.clone().into(), input.task.clone().into(), input.outcome.clone().into(), result.into()]).await?;
        wakeups::cancel(&tx, &job.workspace, &input.task).await?;
        tx.commit().await?;
        Ok(json!({"id": task["id"], "state": input.outcome}))
    }
}

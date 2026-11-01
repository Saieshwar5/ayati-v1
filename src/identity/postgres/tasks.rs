//! Tasks (people-and-agents-first, Phase 2): a task is a promise with an outcome.
//!
//! The platform opens an automatic task when a request changes a real system,
//! waits for someone, or starts from a schedule or event; it closes with that
//! request. An agent opens a commitment with `create_task` when it promises
//! something that outlives its reply, attaches later work with `attach_to_task`
//! and closes it with `close_task`. A person can turn a request into a task.
//! Status is derived from the task's requests (`relay_task_status`), never stored.
use super::*;
use crate::identity::chat::model::Job;
use serde::Deserialize;
use serde_json::json;

#[path = "tasks_migration.rs"]
pub(super) mod migration;
#[cfg(test)]
#[path = "tasks_tests.rs"]
mod tests;
#[path = "task_views.rs"]
pub(super) mod views;

/// Who may see task `t` ($2 the person, $3 admin): the person it is for, anyone who
/// can see its conversation, the agent's managers and admins.
pub(super) const VISIBLE: &str = "(t.requester=$2 OR $3
 OR EXISTS(SELECT 1 FROM relay_agent_managers g WHERE g.workspace=t.workspace AND g.agent=t.agent AND g.user_id=$2)
 OR EXISTS(SELECT 1 FROM relay_chats c WHERE c.workspace=t.workspace AND c.id=t.chat
  AND (c.visibility='public' OR EXISTS(SELECT 1 FROM relay_chat_people p WHERE p.workspace=c.workspace AND p.chat=c.id AND p.user_id=$2))))";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateTask {
    pub title: String,
    pub done_when: Option<String>,
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
/// the same transaction as the write, question, hand-off, schedule run or event
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
           (SELECT s.name FROM relay_schedule_occurrences o JOIN relay_agent_schedules s ON s.workspace=o.workspace AND s.id=o.schedule
             WHERE o.workspace=j.workspace AND o.reply=j.reply),
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

/// A task the agent of `job` may work on: its own, for the same person or in the
/// same conversation.
async fn agent_task(tx: &DatabaseTransaction, job: &Job, id: &str) -> Result<Value> {
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
        tx.commit().await?;
        Ok(
            json!({"id": id, "title": title, "state": "open", "message": "Task saved. Keep working on it now if you can; when its outcome is reached, call close_task."}),
        )
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
        tx.commit().await?;
        Ok(json!({"tasks": items}))
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
        tx.commit().await?;
        Ok(json!({"id": task["id"], "state": input.outcome}))
    }
}

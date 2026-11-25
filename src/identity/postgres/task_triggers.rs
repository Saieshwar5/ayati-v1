//! Event triggers as task wake-ups (schema 148, designs/agent-tasks/one-system.md,
//! decision 6). Each trigger has a task; a delivered event starts a run of it. The
//! trigger keeps its reviewed source, fields and filter, runs in its conversation
//! as its person (schema 151) and is set up on
//! the agent page. A trigger's state reaches its task in the database
//! (`relay_event_trigger_task_state`); the task's own Pause and End reach the
//! trigger here. Resuming needs the trigger's access reviewed again, which only the
//! agent page can do.
use super::super::*;
use crate::identity::chat::model::Job;

/// Why a trigger's task can't be resumed from its task page or in chat.
pub(crate) const RESUME_ON_AGENT_PAGE: &str = "This task runs when its event trigger fires. Resume the trigger on the agent's page (Automations), where its access is reviewed again";

/// Event triggers have tasks from schema 148.
pub(crate) async fn ready(db: &impl ConnectionTrait) -> Result<bool> {
    Ok(query(
        db,
        "SELECT EXISTS(SELECT 1 FROM pg_attribute WHERE attrelid=to_regclass('relay_event_triggers') AND attname='task' AND NOT attisdropped) AS ready",
        vec![],
    )
    .await?
    .context("Schema check unavailable")?
    .try_get("", "ready")?)
}

/// The trigger `task` belongs to and its state, if it is a trigger's task.
pub(in crate::identity::postgres) async fn of_task(
    db: &impl ConnectionTrait,
    w: &str,
    task: &str,
) -> Result<Option<(String, String)>> {
    if !ready(db).await? {
        return Ok(None);
    }
    query(
        db,
        "SELECT id,state FROM relay_event_triggers WHERE workspace=$1 AND task=$2",
        vec![w.into(), task.into()],
    )
    .await?
    .map(|r| Ok((r.try_get("", "id")?, r.try_get("", "state")?)))
    .transpose()
}

/// Make the task of trigger `id`, saved in `tx` (from schema 148).
pub(in crate::identity::postgres) async fn create(
    tx: &DatabaseTransaction,
    w: &str,
    id: &str,
) -> Result<()> {
    if ready(tx).await? {
        query(
            tx,
            "SELECT relay_event_trigger_task($1,$2) AS task",
            vec![w.into(), id.into()],
        )
        .await?;
    }
    Ok(())
}

/// A delivery of trigger `trigger` started `job`: it is the next run of the
/// trigger's task, counted and shown with its runs. False before schema 148.
pub(in crate::identity::postgres) async fn start_run(
    tx: &DatabaseTransaction,
    job: &Job,
    trigger: &str,
    message: &str,
) -> Result<bool> {
    if !ready(tx).await? {
        return Ok(false);
    }
    let w = job.workspace.as_str();
    let Some(task) = query(
        tx,
        "SELECT task FROM relay_event_triggers WHERE workspace=$1 AND id=$2",
        vec![w.into(), trigger.into()],
    )
    .await?
    .and_then(|r| r.try_get::<Option<String>>("", "task").ok().flatten()) else {
        return Ok(false);
    };
    execute(
        tx,
        "UPDATE relay_chat_jobs SET task=$3 WHERE workspace=$1 AND reply=$2",
        vec![w.into(), job.reply.clone().into(), task.clone().into()],
    )
    .await?;
    let run: i32 = query(
        tx,
        "UPDATE relay_tasks SET runs=runs+1 WHERE workspace=$1 AND id=$2 RETURNING runs",
        vec![w.into(), task.clone().into()],
    )
    .await?
    .context("Task unavailable")?
    .try_get("", "runs")?;
    execute(
        tx,
        "INSERT INTO relay_task_wakeups(workspace,id,task,due_ms,kind,state,reply,message,run)
         VALUES($1,$2,$3,(extract(epoch FROM clock_timestamp())*1000)::bigint,'event','started',$4,$5,$6)",
        vec![
            w.into(),
            uuid::Uuid::new_v4().to_string().into(),
            task.into(),
            job.reply.clone().into(),
            message.into(),
            run.into(),
        ],
    )
    .await?;
    Ok(true)
}

/// `task` was paused (on its page or in chat): pause its trigger, if it is on.
pub(in crate::identity::postgres) async fn pause_trigger(
    tx: &DatabaseTransaction,
    w: &str,
    task: &str,
    actor: &str,
) -> Result<()> {
    let Some((id, state)) = of_task(tx, w, task).await? else {
        return Ok(());
    };
    if state != "enabled" {
        return Ok(());
    }
    execute(tx, "UPDATE relay_event_triggers SET state='paused',revision=revision+1,detail='' WHERE workspace=$1 AND id=$2 AND state='enabled'",
        vec![w.into(), id.clone().into()]).await?;
    super::super::event_dispatch::cancel_pending(tx, w, &id).await?;
    audit(tx, actor, Some(w), "agent_event_trigger_paused", &id).await
}

/// A trigger's task resumes only with its trigger, on the agent page.
pub(in crate::identity::postgres) async fn ensure_resumable(
    db: &impl ConnectionTrait,
    w: &str,
    task: &str,
) -> Result<()> {
    if let Some((_, state)) = of_task(db, w, task).await? {
        ensure!(state == "enabled", "{RESUME_ON_AGENT_PAGE}");
    }
    Ok(())
}

/// `task` ended: its trigger is removed for good.
pub(in crate::identity::postgres) async fn end_trigger(
    tx: &DatabaseTransaction,
    w: &str,
    task: &str,
    actor: &str,
) -> Result<()> {
    let Some((id, state)) = of_task(tx, w, task).await? else {
        return Ok(());
    };
    if state == "removed" {
        return Ok(());
    }
    super::super::event_triggers::end(tx, w, &id).await?;
    audit(tx, actor, Some(w), "agent_event_trigger_removed", &id).await
}

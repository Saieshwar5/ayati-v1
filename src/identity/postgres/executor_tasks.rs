//! One-off executor tasks that are not chat replies. The pool offers them as
//! background work; each purpose supplies its access check, its context and
//! where the result is saved. A task never automatically reruns inference.
use super::{
    worker_control::{CLOCK, ControlDenied},
    *,
};
use crate::identity::chat::execution::Prepared;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Purpose {
    /// A draft-only sample of an agent's job (`relay_agent_rehearsals`).
    Sample,
}
impl Purpose {
    fn as_str(self) -> &'static str {
        match self {
            Self::Sample => "sample",
        }
    }
    fn parse(value: &str) -> Result<Self> {
        match value {
            "sample" => Ok(Self::Sample),
            _ => anyhow::bail!("Unknown executor task purpose"),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Task {
    pub id: String,
    pub workspace: String,
    pub purpose: Purpose,
    pub agent: String,
    pub actor: String,
}
impl Task {
    fn actor(&self) -> User {
        User {
            id: self.actor.clone(),
            email: String::new(),
            display_name: None,
        }
    }
}

/// Queue a task for its purpose record. The context's reply ID is the task ID.
pub(super) async fn enqueue(
    tx: &DatabaseTransaction,
    purpose: Purpose,
    actor: &str,
    context: &Prepared,
) -> Result<()> {
    ensure!(
        context.version == 1 && !context.control_tools,
        "Executor tasks are tool-free"
    );
    execute(tx,"INSERT INTO relay_executor_tasks(id,workspace,purpose,agent,actor,model,provider) VALUES($1,$2,$3,$4,$5,$6,$7)",
        vec![context.reply.clone().into(),context.workspace.clone().into(),purpose.as_str().into(),context.agent.clone().into(),actor.into(),context.model.clone().into(),context.provider.clone().into()]).await?;
    Ok(())
}

pub(super) async fn load(db: &impl ConnectionTrait, id: &str) -> Result<Task> {
    let row = query(
        db,
        "SELECT id,workspace,purpose,agent,actor FROM relay_executor_tasks WHERE id=$1",
        vec![id.into()],
    )
    .await?
    .context(ControlDenied)?;
    Ok(Task {
        id: row.try_get("", "id")?,
        workspace: row.try_get("", "workspace")?,
        purpose: Purpose::parse(&row.try_get::<String>("", "purpose")?)?,
        agent: row.try_get("", "agent")?,
        actor: row.try_get("", "actor")?,
    })
}

impl PgStore {
    pub(crate) async fn task_context(&self, task: &Task) -> Result<Prepared> {
        context(&self.db, task).await
    }
}

/// Claim the next eligible task for a ready slot. Called under the admission lock.
pub(super) async fn claim(
    tx: &DatabaseTransaction,
    params: Vec<sea_orm::Value>,
    assignment: &str,
    lease_seconds: u64,
) -> Result<Option<String>> {
    let candidate = query(
        tx,
        concat!(
            include_str!("pool_load.sql"),
            include_str!("pool_task_candidate.sql")
        ),
        params,
    )
    .await?;
    let Some(candidate) = candidate else {
        return Ok(None);
    };
    let id: String = candidate.try_get("", "id")?;
    execute(tx,&format!("UPDATE relay_executor_tasks SET state='running',claim=$2,lease_until={CLOCK}+15 WHERE id=$1 AND state='queued'"),vec![id.clone().into(),assignment.into()]).await?;
    execute(tx,"INSERT INTO relay_worker_assignments(id,workspace,task,generation,provider,state,lease_seconds) VALUES($1,$2,$3,1,'local','launching',$4)",
        vec![assignment.into(),candidate.try_get::<String>("","workspace")?.into(),id.clone().into(),(lease_seconds as i32).into()]).await?;
    Ok(Some(id))
}

/// The task's current assignment still owns an unexpired lease.
pub(super) async fn running(tx: &DatabaseTransaction, task: &Task, claim: &str) -> Result<bool> {
    Ok(query(tx,&format!("SELECT id FROM relay_executor_tasks WHERE id=$1 AND claim=$2 AND state='running' AND lease_until>{CLOCK} FOR UPDATE"),
        vec![task.id.clone().into(),claim.into()]).await?.is_some())
}

/// The requester may still receive this result and its inputs are unchanged.
pub(super) async fn allowed(tx: &DatabaseTransaction, task: &Task) -> Result<bool> {
    match task.purpose {
        Purpose::Sample => super::rehearsal_queue::allowed(tx, task, &task.actor()).await,
    }
}

/// The execution context, rebuilt from the purpose's own saved request.
pub(super) async fn context(db: &impl ConnectionTrait, task: &Task) -> Result<Prepared> {
    match task.purpose {
        Purpose::Sample => super::rehearsal_queue::context(db, task).await,
    }
}

/// First start of an assignment: move it to running and tell the purpose.
pub(super) async fn begin(tx: &DatabaseTransaction, task: &Task, claim: &str) -> Result<()> {
    let started = execute(tx,"UPDATE relay_worker_assignments SET state='running',started_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint,heartbeat_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint WHERE id=$1 AND task=$2 AND state='launching'",
        vec![claim.into(),task.id.clone().into()]).await?;
    ensure!(started == 1, "Assignment already started");
    match task.purpose {
        Purpose::Sample => super::rehearsal_queue::started(tx, task, claim).await,
    }
}

pub(super) async fn refresh(
    tx: &DatabaseTransaction,
    task: &Task,
    claim: &str,
    seconds: u64,
) -> Result<bool> {
    Ok(execute(tx,&format!("UPDATE relay_executor_tasks SET lease_until={CLOCK}+$3 WHERE id=$1 AND claim=$2 AND state='running' AND lease_until>{CLOCK}"),
        vec![task.id.clone().into(),claim.into(),(seconds as i64).into()]).await? == 1)
}

/// Save the executor's result through the purpose and close the assignment.
pub(super) async fn finish(
    tx: &DatabaseTransaction,
    task: &Task,
    claim: &str,
    state: &str,
    text: &str,
) -> Result<()> {
    close(tx, task, claim, state).await?;
    match task.purpose {
        Purpose::Sample => super::rehearsal_queue::finished(tx, task, state, text).await,
    }
}

/// Access or inputs changed: stop the task and mark its purpose record invalid.
/// The caller commits and then denies the executor's request.
pub(super) async fn invalidate(tx: &DatabaseTransaction, task: &Task, claim: &str) -> Result<()> {
    close(tx, task, claim, "cancelled").await?;
    match task.purpose {
        Purpose::Sample => super::rehearsal_queue::invalidate(tx, &task.workspace, &task.id).await,
    }
}

/// Stopped by its requester through the purpose's own Stop action.
pub(super) async fn cancel(tx: &DatabaseTransaction, id: &str) -> Result<()> {
    let row = query(
        tx,
        "SELECT claim FROM relay_executor_tasks WHERE id=$1 AND state<>'finished' FOR UPDATE",
        vec![id.into()],
    )
    .await?;
    let Some(row) = row else { return Ok(()) };
    finished(tx, id).await?;
    if let Some(claim) = row.try_get::<Option<String>>("", "claim")? {
        end_assignment(tx, &claim, "cancelled").await?;
    }
    Ok(())
}

async fn close(tx: &DatabaseTransaction, task: &Task, claim: &str, outcome: &str) -> Result<()> {
    finished(tx, &task.id).await?;
    end_assignment(tx, claim, outcome).await
}

async fn finished(tx: &DatabaseTransaction, id: &str) -> Result<()> {
    execute(tx,"UPDATE relay_executor_tasks SET state='finished',claim=NULL,lease_until=0,finished_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint WHERE id=$1 AND state<>'finished'",vec![id.into()]).await?;
    Ok(())
}

async fn end_assignment(tx: &DatabaseTransaction, claim: &str, outcome: &str) -> Result<()> {
    execute(tx,"UPDATE relay_worker_assignments SET state='finished',outcome=$2,finished_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint WHERE id=$1 AND state IN ('launching','running')",vec![claim.into(),outcome.into()]).await?;
    if outcome == "cancelled" {
        execute(
            tx,
            "UPDATE relay_assignment_credentials SET context=NULL,expires=0 WHERE assignment=$1",
            vec![claim.into()],
        )
        .await?;
    }
    Ok(())
}

/// Expired leases end the task as interrupted; nothing is rerun. Queued tasks
/// whose purpose record was stopped or invalidated are closed.
pub(super) async fn recover(tx: &DatabaseTransaction) -> Result<()> {
    let expired = rows(tx,&format!("SELECT id,claim FROM relay_executor_tasks WHERE state='running' AND lease_until<={CLOCK} FOR UPDATE SKIP LOCKED"),vec![]).await?;
    for row in expired {
        let task = load(tx, &row.try_get::<String>("", "id")?).await?;
        let claim: String = row.try_get("", "claim")?;
        execute(tx,"UPDATE relay_worker_assignments SET state='lost',outcome='lost',finished_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint WHERE id=$1 AND state IN ('launching','running')",vec![claim.into()]).await?;
        finished(tx, &task.id).await?;
        match task.purpose {
            Purpose::Sample => super::rehearsal_queue::interrupted(tx, &task).await?,
        }
    }
    execute(tx,"UPDATE relay_executor_tasks t SET state='finished',finished_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint WHERE t.state='queued' AND t.purpose='sample'
        AND NOT EXISTS(SELECT 1 FROM relay_agent_rehearsals r WHERE r.workspace=t.workspace AND r.id=t.id AND r.state='queued')",vec![]).await?;
    Ok(())
}

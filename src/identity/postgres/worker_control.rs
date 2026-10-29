//! Durable local executor registration and assignment capabilities, stored only as hashes.
use super::*;
use crate::identity::worker_control::model::{Registration, Scope};
pub(super) const CLOCK: &str = "floor(extract(epoch FROM clock_timestamp()))::bigint";

#[derive(Debug)]
pub(crate) struct ControlDenied;
impl std::fmt::Display for ControlDenied {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Machine credential or assignment scope denied")
    }
}
impl std::error::Error for ControlDenied {}
pub(crate) fn require(allowed: bool) -> Result<()> {
    if allowed {
        Ok(())
    } else {
        Err(ControlDenied.into())
    }
}

pub(crate) struct Controlled {
    pub scope: Scope,
    pub assignment: super::assignments::Assignment,
    pub session: Option<String>,
    pub context: Option<Value>,
    pub completion_hash: Option<String>,
    pub completion_state: Option<String>,
}

impl PgStore {
    pub(crate) async fn register_executor(&self, registration: &Registration) -> Result<()> {
        let tx = self.db.begin().await?;
        super::cloud_executors::register_in(&tx, registration, false).await?;
        require(
            query(
                &tx,
                "SELECT executor FROM relay_cloud_executors WHERE executor=$1",
                vec![registration.executor.clone().into()],
            )
            .await?
            .is_none(),
        )?;
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn executor_allowed(
        &self,
        executor: &str,
        incarnation: &str,
        hash: &str,
    ) -> Result<()> {
        require(query(&self.db, &format!("SELECT id FROM relay_executors WHERE id=$1 AND incarnation=$2 AND token_hash=$3 AND state IN ('serving','draining') AND expires>{CLOCK}"),
            vec![executor.into(),incarnation.into(),hash.into()]).await?.is_some())
    }

    pub(crate) async fn controlled_assignment(
        &self,
        scope: &Scope,
        hash: &str,
    ) -> Result<Controlled> {
        let record = controlled(&self.db, &scope.assignment, hash).await?;
        require(&record.scope == scope)?;
        Ok(record)
    }
}

const SCOPE_QUERY: &str =
    "SELECT c.executor,c.incarnation,c.slot,c.slot_generation,a.id AS assignment,a.workspace,a.generation,
    a.reply,j.payload->>'agent' AS agent,j.payload->>'chat' AS chat,m.thread
    FROM relay_assignment_credentials c JOIN relay_worker_assignments a ON a.id=c.assignment
    JOIN relay_chat_jobs j ON j.reply=a.reply AND j.workspace=a.workspace
    JOIN relay_chat_messages m ON m.id=a.reply AND m.workspace=a.workspace WHERE c.assignment=$1";
fn scope_from_row(row: QueryResult) -> Result<Scope> {
    Ok(Scope {
        executor: row.try_get("", "executor")?,
        incarnation: row.try_get("", "incarnation")?,
        slot: row.try_get("", "slot")?,
        slot_generation: row.try_get("", "slot_generation")?,
        assignment: row.try_get("", "assignment")?,
        workspace: row.try_get("", "workspace")?,
        agent: row.try_get("", "agent")?,
        chat: row.try_get("", "chat")?,
        thread: row.try_get("", "thread")?,
        reply: row.try_get("", "reply")?,
        generation: row.try_get("", "generation")?,
    })
}

pub(super) async fn controlled(
    db: &impl ConnectionTrait,
    id: &str,
    hash: &str,
) -> Result<Controlled> {
    let row = query(db, &format!("SELECT j.payload,j.run_seconds,a.lease_seconds,c.session,c.context,c.completion_hash,c.completion_state
        FROM relay_assignment_credentials c JOIN relay_executors e ON e.id=c.executor AND e.incarnation=c.incarnation
        JOIN relay_worker_assignments a ON a.id=c.assignment JOIN relay_chat_jobs j ON j.reply=a.reply AND j.workspace=a.workspace
        WHERE c.assignment=$1 AND c.token_hash=$2 AND c.expires>{CLOCK} AND e.expires>{CLOCK} AND e.state IN ('serving','draining')"), vec![id.into(),hash.into()]).await?.context(ControlDenied)?;
    let scope = scope_from_row(
        query(db, SCOPE_QUERY, vec![id.into()])
            .await?
            .context(ControlDenied)?,
    )?;
    let mut job: crate::identity::chat::store::Job =
        serde_json::from_value(row.try_get("", "payload")?)?;
    job.claim = Some(id.into());
    Ok(Controlled {
        scope,
        assignment: super::assignments::Assignment {
            job,
            lease_seconds: row.try_get::<i32>("", "lease_seconds")? as u64,
            run_seconds: row.try_get::<i32>("", "run_seconds")? as u64,
        },
        session: row.try_get("", "session")?,
        context: row.try_get("", "context")?,
        completion_hash: row.try_get("", "completion_hash")?,
        completion_state: row.try_get("", "completion_state")?,
    })
}

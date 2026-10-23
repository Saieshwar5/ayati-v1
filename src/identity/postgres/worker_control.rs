//! Durable local executor registration and assignment capabilities, stored only as hashes.
use super::*;
use crate::identity::worker_control::model::{self, Registration, Reservation, Scope};
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
        registration.validate()?;
        let changed = execute(
            &self.db,
            &format!(
                "INSERT INTO relay_executors(id,incarnation,token_hash,expires)
            VALUES($1,$2,$3,{CLOCK}+86400) ON CONFLICT(id) DO UPDATE SET expires={CLOCK}+86400
            WHERE relay_executors.incarnation=$2 AND relay_executors.token_hash=$3"
            ),
            vec![
                registration.executor.clone().into(),
                registration.incarnation.clone().into(),
                super::super::provider::digest(&registration.token).into(),
            ],
        )
        .await?;
        require(changed == 1)
    }

    pub(crate) async fn executor_allowed(
        &self,
        executor: &str,
        incarnation: &str,
        hash: &str,
    ) -> Result<()> {
        require(query(&self.db, &format!("SELECT id FROM relay_executors WHERE id=$1 AND incarnation=$2 AND token_hash=$3 AND expires>{CLOCK}"),
            vec![executor.into(),incarnation.into(),hash.into()]).await?.is_some())
    }

    pub(crate) async fn reserve_controlled(
        &self,
        executor: &str,
        incarnation: &str,
        request: &Reservation,
        lease: u64,
        capacity: usize,
    ) -> Result<Option<Scope>> {
        model::uuid(&request.request)?;
        model::token(&request.token)?;
        ensure!(
            (6..=300).contains(&lease) && (1..=16).contains(&capacity),
            "Invalid assignment policy"
        );
        let hash = super::super::provider::digest(&request.token);
        let tx = self.db.begin().await?;
        super::assignments::admission_gate(&tx).await?;
        if let Some(row) = query(&tx, "SELECT assignment,token_hash FROM relay_assignment_credentials WHERE executor=$1 AND incarnation=$2 AND request=$3",
            vec![executor.into(),incarnation.into(),request.request.clone().into()]).await? {
            require(row.try_get::<String>("", "token_hash")? == hash)?;
            let id: String = row.try_get("", "assignment")?;
            // Executor authentication permits replaying this reservation's metadata
            // even after the run capability expires; it never revives that capability.
            let scope = scope_from_row(query(&tx,SCOPE_QUERY,vec![id.into()]).await?.context(ControlDenied)?)?;
            tx.commit().await?;
            return Ok(Some(scope));
        }
        let Some(id) = self.reserve_assignment_in(&tx, lease, capacity).await? else {
            tx.commit().await?;
            return Ok(None);
        };
        execute(&tx, &format!("INSERT INTO relay_assignment_credentials(assignment,executor,incarnation,slot,request,token_hash,expires)
            VALUES($1,$2,$3,$4,$5,$6,{CLOCK}+300)"), vec![id.clone().into(), executor.into(),incarnation.into(),
                uuid::Uuid::new_v4().to_string().into(),request.request.clone().into(),hash.clone().into()]).await?;
        let record = controlled(&tx, &id, &hash).await?;
        execute(
            &tx,
            "UPDATE relay_worker_assignments SET resource_id=$2 WHERE id=$1",
            vec![id.into(), record.scope.resource().into()],
        )
        .await?;
        tx.commit().await?;
        Ok(Some(record.scope))
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

    pub(crate) async fn release_controlled(
        &self,
        executor: &str,
        incarnation: &str,
        scope: &Scope,
        reason: &str,
    ) -> Result<()> {
        require(scope.executor == executor && scope.incarnation == incarnation)?;
        // Launcher credentials may reconcile an expired run capability after its child has exited.
        let stored = scope_from_row(
            query(&self.db, SCOPE_QUERY, vec![scope.assignment.clone().into()])
                .await?
                .context(ControlDenied)?,
        )?;
        require(stored == *scope)?;
        self.release_assignment(&scope.assignment, reason, Some(&scope.resource()))
            .await
    }
}

const SCOPE_QUERY: &str =
    "SELECT c.executor,c.incarnation,c.slot,a.id AS assignment,a.workspace,a.generation,
    a.reply,j.payload->>'agent' AS agent,j.payload->>'chat' AS chat,m.thread
    FROM relay_assignment_credentials c JOIN relay_worker_assignments a ON a.id=c.assignment
    JOIN relay_chat_jobs j ON j.reply=a.reply AND j.workspace=a.workspace
    JOIN relay_chat_messages m ON m.id=a.reply AND m.workspace=a.workspace WHERE c.assignment=$1";
fn scope_from_row(row: QueryResult) -> Result<Scope> {
    Ok(Scope {
        executor: row.try_get("", "executor")?,
        incarnation: row.try_get("", "incarnation")?,
        slot: row.try_get("", "slot")?,
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
        WHERE c.assignment=$1 AND c.token_hash=$2 AND c.expires>{CLOCK} AND e.expires>{CLOCK}"), vec![id.into(),hash.into()]).await?.context(ControlDenied)?;
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
            id: id.into(),
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

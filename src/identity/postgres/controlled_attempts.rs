//! Start/renew/finish share the existing assignment transaction and authority rules.
use super::{
    worker_control::{CLOCK, controlled, require},
    *,
};
use crate::identity::{
    chat::execution::Prepared,
    worker_control::model::{AssignedRequest, Completion},
};

impl PgStore {
    pub(crate) async fn start_controlled(
        &self,
        request: &AssignedRequest,
        hash: &str,
        context: &Prepared,
    ) -> Result<Prepared> {
        let scope = &request.scope;
        let tx = self.workspace_tx(&scope.workspace).await?;
        // Workspace serialization also orders access revocation against context delivery.
        let record = controlled(&tx, &scope.assignment, hash).await?;
        require(record.scope == *scope)?;
        let prepared = if let Some(session) = record.session {
            require(session == request.session)?;
            require(active(&tx, &record.assignment.job).await?)?;
            access(&tx, &record.assignment.job).await?;
            serde_json::from_value(record.context.context("Missing execution context")?)?
        } else {
            access(&tx, &record.assignment.job).await?;
            require(
                self.start_assignment_in(&tx, &scope.assignment, &scope.resource())
                    .await?
                    .is_some(),
            )?;
            execute(&tx,&format!("UPDATE relay_assignment_credentials SET session=$2,context=$3,expires={CLOCK}+300 WHERE assignment=$1"),
                vec![scope.assignment.clone().into(),request.session.clone().into(),serde_json::to_value(context)?.into()]).await?;
            self.acknowledge_slot(&tx, scope).await?;
            execute(&tx,"UPDATE relay_worker_assignments SET context_ready_ms=COALESCE(context_ready_ms,(extract(epoch FROM clock_timestamp())*1000)::bigint) WHERE id=$1",vec![scope.assignment.clone().into()]).await?;
            context.clone()
        };
        // Initial start and exact replay confirm a full lease in the same commit
        // as context/session ownership. No second authorization transaction is needed.
        refresh_lease(&tx, scope, record.assignment.lease_seconds).await?;
        tx.commit().await?;
        Ok(prepared)
    }

    pub(crate) async fn renew_controlled(
        &self,
        request: &AssignedRequest,
        hash: &str,
    ) -> Result<u64> {
        let scope = &request.scope;
        let tx = self.workspace_tx(&scope.workspace).await?;
        let record = controlled(&tx, &scope.assignment, hash).await?;
        require(record.scope == *scope && record.session.as_deref() == Some(&request.session))?;
        access(&tx, &record.assignment.job).await?;
        refresh_lease(&tx, scope, record.assignment.lease_seconds).await?;
        tx.commit().await?;
        Ok(record.assignment.lease_seconds)
    }

    pub(crate) async fn complete_controlled(
        &self,
        request: &Completion,
        hash: &str,
    ) -> Result<String> {
        request.validate()?;
        let scope = &request.assignment.scope;
        let tx = self.workspace_tx(&scope.workspace).await?;
        let record = controlled(&tx, &scope.assignment, hash).await?;
        require(
            record.scope == *scope
                && record.session.as_deref() == Some(&request.assignment.session),
        )?;
        let fingerprint = super::super::provider::digest(&serde_json::to_string(request)?);
        if let Some(saved) = record.completion_hash {
            ensure!(saved == fingerprint, "Completion payload changed");
            return record
                .completion_state
                .context("Missing completion receipt");
        }
        require(
            self.finish_job_in(&tx, &record.assignment.job, &request.text, &request.state)
                .await?,
        )?;
        let state: String = query(
            &tx,
            "SELECT outcome FROM relay_worker_assignments WHERE id=$1",
            vec![scope.assignment.clone().into()],
        )
        .await?
        .context("Missing outcome")?
        .try_get("", "outcome")?;
        execute(&tx,&format!("UPDATE relay_assignment_credentials SET completion_hash=$2,completion_state=$3,context=NULL,expires={CLOCK}+300 WHERE assignment=$1"),
            vec![scope.assignment.clone().into(),fingerprint.into(),state.clone().into()]).await?;
        execute(
            &tx,
            "UPDATE relay_executor_slots SET state='cleaning' WHERE assignment=$1",
            vec![scope.assignment.clone().into()],
        )
        .await?;
        tx.commit().await?;
        Ok(state)
    }
}

async fn active(tx: &DatabaseTransaction, job: &crate::identity::chat::store::Job) -> Result<bool> {
    Ok(query(tx,&format!("SELECT reply FROM relay_chat_jobs WHERE workspace=$1 AND reply=$2 AND claim=$3 AND state='running' AND lease_until>{CLOCK}"),
        vec![job.workspace.clone().into(),job.reply.clone().into(),job.claim.clone().into()]).await?.is_some())
}
pub(super) async fn access(
    tx: &DatabaseTransaction,
    job: &crate::identity::chat::store::Job,
) -> Result<()> {
    let user = User {
        id: job.requester.clone(),
        email: String::new(),
        display_name: None,
    };
    require(
        super::chat::access(tx, &user, &job.workspace, &job.chat)
            .await
            .is_ok()
            && super::chat::agent_access(tx, &user.id, &job.workspace, &job.agent).await?
            && super::source_access::allowed(tx, job).await?
            && super::context_attempts::allowed(tx, job).await?
            && super::handoff_access::execution_allowed(tx, job).await?,
    )
}

// Callers hold the workspace lock and have checked the exact capability, session
// and current access. This helper only performs the owned lease writes.
async fn refresh_lease(
    tx: &DatabaseTransaction,
    scope: &crate::identity::worker_control::model::Scope,
    seconds: u64,
) -> Result<()> {
    let renewed=execute(tx,&format!("UPDATE relay_chat_jobs SET lease_until={CLOCK}+$3 WHERE workspace=$1 AND claim=$2 AND state='running' AND lease_until>{CLOCK}"),
        vec![scope.workspace.clone().into(),scope.assignment.clone().into(),(seconds as i64).into()]).await?;
    require(renewed == 1)?;
    execute(tx,"UPDATE relay_worker_assignments SET heartbeat_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint WHERE id=$1 AND state='running'",vec![scope.assignment.clone().into()]).await?;
    execute(
        tx,
        &format!("UPDATE relay_assignment_credentials SET expires={CLOCK}+300 WHERE assignment=$1"),
        vec![scope.assignment.clone().into()],
    )
    .await?;
    execute(
        tx,
        &format!("UPDATE relay_executors SET expires={CLOCK}+86400 WHERE id=$1 AND incarnation=$2"),
        vec![
            scope.executor.clone().into(),
            scope.incarnation.clone().into(),
        ],
    )
    .await?;
    Ok(())
}

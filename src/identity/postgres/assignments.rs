//! Fenced pooled assignment attempts. No provider calls in transactions.
use super::*;
use crate::identity::chat::store::Job;

#[derive(Clone, Debug)]
pub(crate) struct Assignment {
    pub job: Job,
    pub lease_seconds: u64,
    pub run_seconds: u64,
}

pub(super) async fn admission_gate(tx: &DatabaseTransaction) -> Result<()> {
    query(tx, "SELECT pg_advisory_xact_lock(hashtextextended(current_database() || '/' || current_schema() || '/relay_assignments',0))", vec![]).await?;
    Ok(())
}

impl PgStore {
    pub(super) async fn recover_assignments_in(&self, tx: &DatabaseTransaction) -> Result<()> {
        // Shared by cooperating local launchers; never held over spawn/model calls.
        admission_gate(tx).await?;
        execute(tx, r#"WITH expired AS (
            SELECT j.claim FROM relay_chat_jobs j WHERE j.dispatch IN ('assigned','pooled')
            AND j.state='running' AND j.lease_until<=floor(extract(epoch FROM clock_timestamp()))::bigint
            FOR UPDATE OF j SKIP LOCKED)
            UPDATE relay_worker_assignments a SET state='lost',outcome='lost',
            finished_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint
            FROM expired WHERE expired.claim=a.id AND a.state IN ('launching','running')"#, vec![]).await?;
        execute(tx,"UPDATE relay_assignment_credentials c SET context=NULL,expires=0 FROM relay_worker_assignments a WHERE a.id=c.assignment AND a.state='lost' AND c.expires<>0",vec![]).await?;
        execute(tx,"UPDATE relay_executor_slots s SET state='lost' FROM relay_worker_assignments a WHERE a.id=s.assignment AND a.state='lost' AND s.state<>'lost'",vec![]).await?;
        let exhausted = rows(
            tx,
            r#"UPDATE relay_chat_jobs SET state='finished',
            finished_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint
            WHERE dispatch IN ('assigned','pooled') AND state='running' AND attempts>=3
            AND lease_until<=floor(extract(epoch FROM clock_timestamp()))::bigint RETURNING reply"#,
            vec![],
        )
        .await?;
        for row in exhausted {
            execute(tx, "UPDATE relay_chat_messages SET state='failed',content='The agent could not recover this assignment. Send a new request to retry.' WHERE id=$1 AND state='queued'", vec![row.try_get::<String>("", "reply")?.into()]).await?;
        }
        Ok(())
    }

    pub(super) async fn start_assignment_in(
        &self,
        tx: &DatabaseTransaction,
        id: &str,
        resource: &str,
    ) -> Result<Option<Assignment>> {
        let row = query(tx, r#"SELECT j.payload,j.run_seconds,a.lease_seconds FROM relay_chat_jobs j
            JOIN relay_worker_assignments a ON a.id=j.claim AND a.reply=j.reply AND a.workspace=j.workspace
            WHERE a.id=$1 AND a.state='launching' AND a.provider='local' AND j.dispatch IN ('assigned','pooled')
            AND j.state='running' AND j.lease_until>floor(extract(epoch FROM clock_timestamp()))::bigint
            AND (a.resource_id IS NULL OR a.resource_id=$2) FOR UPDATE OF j"#, vec![id.into(),resource.into()]).await?;
        let Some(row) = row else { return Ok(None) };
        let started = execute(
            tx,
            r#"UPDATE relay_worker_assignments SET state='running',resource_id=$2,
            started_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint,
            heartbeat_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint
            WHERE id=$1 AND state='launching'"#,
            vec![id.into(), resource.into()],
        )
        .await?;
        ensure!(started == 1, "Assignment already started");
        let lease_seconds = row.try_get::<i32>("", "lease_seconds")? as u64;
        execute(tx, "UPDATE relay_chat_jobs SET lease_until=floor(extract(epoch FROM clock_timestamp()))::bigint+$2,started_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint WHERE claim=$1 AND state='running'", vec![id.into(),(lease_seconds as i64).into()]).await?;
        let mut job: Job = serde_json::from_value(row.try_get::<Value>("", "payload")?)?;
        job.claim = Some(id.into());
        let assignment = Assignment {
            job,
            lease_seconds,
            run_seconds: row.try_get::<i32>("", "run_seconds")? as u64,
        };
        Ok(Some(assignment))
    }

    pub(crate) async fn cancel_assignment(
        &self,
        user: &User,
        workspace: &str,
        chat: &str,
        reply: &str,
    ) -> Result<()> {
        let tx = self.workspace_tx(workspace).await?;
        super::chat::access(&tx, user, workspace, chat).await?;
        let row = query(&tx, "SELECT requester FROM relay_chat_messages WHERE workspace=$1 AND chat=$2 AND id=$3 AND kind='agent'", vec![workspace.into(),chat.into(),reply.into()]).await?.context("Assignment unavailable")?;
        ensure!(
            row.try_get::<String>("", "requester")? == user.id
                || role(&tx, workspace, &user.id).await? == "Owner",
            "Only the requester or owner can cancel this request"
        );
        let row = query(&tx, "UPDATE relay_chat_jobs SET state='finished',lease_until=0,finished_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint WHERE workspace=$1 AND reply=$2 AND state!='finished' RETURNING claim", vec![workspace.into(),reply.into()]).await?;
        if let Some(row) = row {
            execute(&tx,"UPDATE relay_control_actions SET state='invalidated',result='\"Original task was stopped; no pending operation was dispatched.\"'::jsonb WHERE workspace=$1 AND job=$2 AND state='pending'",vec![workspace.into(),reply.into()]).await?;
            if let Some(claim) = row.try_get::<Option<String>>("", "claim")? {
                execute(&tx, "UPDATE relay_worker_assignments SET state='finished',outcome='cancelled',finished_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint WHERE id=$1 AND state IN ('launching','running')", vec![claim.clone().into()]).await?;
                execute(&tx,"UPDATE relay_assignment_credentials SET context=NULL,expires=0 WHERE assignment=$1",vec![claim.into()]).await?;
            }
            execute(&tx, "UPDATE relay_chat_messages SET state='cancelled',content='Request cancelled.' WHERE workspace=$1 AND id=$2 AND state='queued'", vec![workspace.into(),reply.into()]).await?;
            audit(
                &tx,
                &user.id,
                Some(workspace),
                "agent_request_cancelled",
                reply,
            )
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }
}

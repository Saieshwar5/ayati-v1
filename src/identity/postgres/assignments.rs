//! Durable local launch intents and fenced assignment attempts. No provider calls in transactions.
use super::*;
use crate::identity::chat::store::Job;

#[derive(Clone, Debug)]
pub(crate) struct Assignment {
    pub id: String,
    pub job: Job,
    pub lease_seconds: u64,
    pub run_seconds: u64,
}

pub(super) async fn admission_gate(tx: &DatabaseTransaction) -> Result<()> {
    query(tx, "SELECT pg_advisory_xact_lock(hashtextextended(current_database() || '/' || current_schema() || '/relay_assignments',0))", vec![]).await?;
    Ok(())
}

impl PgStore {
    pub(crate) async fn reserve_assignment(
        &self,
        lease: u64,
        capacity: usize,
    ) -> Result<Option<String>> {
        ensure!(
            (6..=300).contains(&lease) && (1..=16).contains(&capacity),
            "Invalid assignment limits"
        );
        let tx = self.db.begin().await?;
        let result = self.reserve_assignment_in(&tx, lease, capacity).await?;
        tx.commit().await?;
        Ok(result)
    }

    pub(super) async fn reserve_assignment_in(
        &self,
        tx: &DatabaseTransaction,
        lease: u64,
        capacity: usize,
    ) -> Result<Option<String>> {
        // Shared by cooperating local launchers; never held over spawn/model calls.
        admission_gate(tx).await?;
        execute(tx, r#"WITH expired AS (
            SELECT j.claim FROM relay_chat_jobs j WHERE j.dispatch='assigned'
            AND j.state='running' AND j.lease_until<=floor(extract(epoch FROM clock_timestamp()))::bigint
            FOR UPDATE OF j SKIP LOCKED)
            UPDATE relay_worker_assignments a SET state='lost',outcome='lost',
            finished_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint
            FROM expired WHERE expired.claim=a.id AND a.state IN ('launching','running')"#, vec![]).await?;
        execute(tx,"UPDATE relay_assignment_credentials c SET context=NULL,expires=0 FROM relay_worker_assignments a WHERE a.id=c.assignment AND a.state='lost' AND c.expires<>0",vec![]).await?;
        let exhausted = rows(
            tx,
            r#"UPDATE relay_chat_jobs SET state='finished',
            finished_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint
            WHERE dispatch='assigned' AND state='running' AND attempts>=3
            AND lease_until<=floor(extract(epoch FROM clock_timestamp()))::bigint RETURNING reply"#,
            vec![],
        )
        .await?;
        for row in exhausted {
            execute(tx, "UPDATE relay_chat_messages SET state='failed',content='The agent could not recover this assignment. Send a new request to retry.' WHERE id=$1 AND state='queued'", vec![row.try_get::<String>("", "reply")?.into()]).await?;
        }
        let active: i64 = query(tx, "SELECT count(*) AS n FROM relay_chat_jobs WHERE dispatch='assigned' AND state='running' AND lease_until>floor(extract(epoch FROM clock_timestamp()))::bigint", vec![]).await?.context("Assignment count unavailable")?.try_get("", "n")?;
        if active >= capacity as i64 {
            return Ok(None);
        }
        let row = query(tx, r#"SELECT j.reply FROM relay_chat_jobs j
            WHERE j.dispatch='assigned' AND j.attempts<3
            AND (j.state='queued' OR (j.state='running' AND j.lease_until<=floor(extract(epoch FROM clock_timestamp()))::bigint))
            AND NOT EXISTS (SELECT 1 FROM relay_chat_jobs other WHERE other.workspace=j.workspace
                AND other.payload->>'agent'=j.payload->>'agent' AND other.reply<>j.reply
                AND other.state='running' AND other.lease_until>floor(extract(epoch FROM clock_timestamp()))::bigint)
            ORDER BY j.sequence FOR UPDATE OF j SKIP LOCKED LIMIT 1"#, vec![]).await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let reply: String = row.try_get("", "reply")?;
        let id = uuid::Uuid::new_v4().to_string();
        let job = query(
            tx,
            r#"UPDATE relay_chat_jobs SET state='running',claim=$2,attempts=attempts+1,
            lease_until=floor(extract(epoch FROM clock_timestamp()))::bigint+300
            WHERE reply=$1 RETURNING workspace,attempts"#,
            vec![reply.clone().into(), id.clone().into()],
        )
        .await?
        .context("Assignment disappeared")?;
        execute(tx, "INSERT INTO relay_worker_assignments(id,workspace,reply,generation,provider,state,lease_seconds) VALUES($1,$2,$3,$4,'local','launching',$5)", vec![id.clone().into(),job.try_get::<String>("", "workspace")?.into(),reply.into(),job.try_get::<i32>("", "attempts")?.into(),(lease as i32).into()]).await?;
        Ok(Some(id))
    }

    pub(crate) async fn start_assignment(
        &self,
        id: &str,
        resource: &str,
    ) -> Result<Option<Assignment>> {
        let tx = self.db.begin().await?;
        let result = self.start_assignment_in(&tx, id, resource).await?;
        tx.commit().await?;
        Ok(result)
    }

    pub(super) async fn start_assignment_in(
        &self,
        tx: &DatabaseTransaction,
        id: &str,
        resource: &str,
    ) -> Result<Option<Assignment>> {
        let row = query(tx, r#"SELECT j.payload,j.run_seconds,a.lease_seconds FROM relay_chat_jobs j
            JOIN relay_worker_assignments a ON a.id=j.claim AND a.reply=j.reply AND a.workspace=j.workspace
            WHERE a.id=$1 AND a.state='launching' AND a.provider='local' AND j.dispatch='assigned'
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
            id: id.into(),
            job,
            lease_seconds,
            run_seconds: row.try_get::<i32>("", "run_seconds")? as u64,
        };
        Ok(Some(assignment))
    }

    pub(crate) async fn record_assignment_resource(
        &self,
        id: &str,
        resource: &str,
    ) -> Result<bool> {
        Ok(execute(&self.db, "UPDATE relay_worker_assignments SET resource_id=$2 WHERE id=$1 AND (resource_id IS NULL OR resource_id=$2)", vec![id.into(),resource.into()]).await? == 1)
    }

    pub(crate) async fn renew_assignment(&self, assignment: &Assignment) -> Result<bool> {
        if !self.chat_job_allowed(&assignment.job).await {
            return Ok(false);
        }
        let tx = self.db.begin().await?;
        let changed = execute(&tx, r#"UPDATE relay_chat_jobs j SET lease_until=floor(extract(epoch FROM clock_timestamp()))::bigint+$3
            WHERE j.workspace=$1 AND j.claim=$2 AND j.state='running'
            AND j.lease_until>floor(extract(epoch FROM clock_timestamp()))::bigint
            AND EXISTS(SELECT 1 FROM relay_worker_assignments a WHERE a.id=j.claim AND a.state='running')"#,
            vec![assignment.job.workspace.clone().into(),assignment.id.clone().into(),(assignment.lease_seconds as i64).into()]).await?;
        if changed != 1 {
            return Ok(false);
        }
        execute(&tx, "UPDATE relay_worker_assignments SET heartbeat_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint WHERE id=$1 AND state='running'", vec![assignment.id.clone().into()]).await?;
        tx.commit().await?;
        Ok(true)
    }

    /// Call only after the local child and its execution have stopped, or spawn failed.
    pub(crate) async fn release_assignment(
        &self,
        id: &str,
        outcome: &str,
        resource: Option<&str>,
    ) -> Result<()> {
        ensure!(
            ["interrupted", "lost", "launch_failed"].contains(&outcome),
            "Invalid release reason"
        );
        let tx = self.db.begin().await?;
        // A losing/unknown spawn must never release another registered process.
        if query(&tx, r#"SELECT j.reply FROM relay_chat_jobs j JOIN relay_worker_assignments a ON a.id=j.claim
            WHERE a.id=$1 AND j.state='running' AND j.dispatch='assigned'
            AND (($2::text IS NULL AND a.resource_id IS NULL AND a.state='launching') OR a.resource_id=$2)
            FOR UPDATE OF j"#, vec![id.into(),resource.into()]).await?.is_none() {
            return Ok(());
        }
        execute(&tx, "UPDATE relay_chat_jobs SET lease_until=0 WHERE claim=$1 AND dispatch='assigned' AND state='running'", vec![id.into()]).await?;
        execute(&tx, "UPDATE relay_worker_assignments SET state='lost',outcome=$2,finished_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint WHERE id=$1 AND state IN ('launching','running')", vec![id.into(),outcome.into()]).await?;
        execute(
            &tx,
            "UPDATE relay_assignment_credentials SET context=NULL,expires=0 WHERE assignment=$1",
            vec![id.into()],
        )
        .await?;
        tx.commit().await?;
        Ok(())
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

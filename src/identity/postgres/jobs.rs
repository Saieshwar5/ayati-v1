use super::*;
use crate::identity::chat::model::Job;
impl PgStore {
    pub async fn claim_job(&self) -> Result<Option<Job>> {
        let tx = self.db.begin().await?;
        super::assignments::admission_gate(&tx).await?;
        // Conversation-only generation may retry. Tool-enabled jobs need action receipts first.
        let exhausted=rows(&tx,"UPDATE relay_chat_jobs SET state='finished' WHERE dispatch='polling' AND state='running' AND lease_until<=floor(extract(epoch FROM clock_timestamp()))::bigint AND attempts>=3 RETURNING reply",vec![]).await?;
        for row in exhausted {
            execute(&tx,"UPDATE relay_chat_messages SET state='failed',content='The agent could not recover this request. Send a new request to retry.' WHERE id=$1 AND state='queued'",vec![row.try_get::<String>("","reply")?.into()]).await?;
        }
        let claim = uuid::Uuid::new_v4().to_string();
        let row=query(&tx,r#"WITH ready AS (
            SELECT j.reply FROM relay_chat_jobs j WHERE j.dispatch='polling'
            AND (j.state='queued' OR (j.state='running' AND j.lease_until<=floor(extract(epoch FROM clock_timestamp()))::bigint))
            AND j.attempts<3 AND NOT EXISTS (
                SELECT 1 FROM relay_chat_jobs other WHERE other.dispatch IN ('assigned','pooled')
                AND other.workspace=j.workspace AND other.payload->>'agent'=j.payload->>'agent'
                AND other.state='running' AND other.lease_until>floor(extract(epoch FROM clock_timestamp()))::bigint)
            ORDER BY j.sequence FOR UPDATE OF j SKIP LOCKED LIMIT 1)
            UPDATE relay_chat_jobs j SET state='running',claim=$1,
            lease_until=floor(extract(epoch FROM clock_timestamp()))::bigint+300,
            attempts=attempts+1,generation=generation+1,started_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint
            FROM ready WHERE j.reply=ready.reply RETURNING j.payload"#,vec![claim.clone().into()]).await?;
        let job = row
            .map(|row| -> Result<Job> {
                let payload: Value = row.try_get("", "payload")?;
                let mut job: Job = serde_json::from_value(payload)?;
                job.claim = Some(claim);
                Ok(job)
            })
            .transpose()?;
        tx.commit().await?;
        Ok(job)
    }
    pub(super) async fn claim_active(&self, job: &Job) -> Result<bool> {
        let Some(claim) = &job.claim else {
            return Ok(false);
        };
        Ok(query(&self.db,"SELECT reply FROM relay_chat_jobs WHERE workspace=$1 AND reply=$2 AND claim=$3 AND state='running' AND lease_until>floor(extract(epoch FROM clock_timestamp()))::bigint",vec![job.workspace.clone().into(),job.reply.clone().into(),claim.clone().into()]).await?.is_some())
    }

    /// Expire this exact claim only after its runtime stops. Retaining `running`
    /// lets retry exhaustion finish attempt 3 instead of stranding a queued job.
    pub(crate) async fn release_job(&self, job: &Job) -> Result<()> {
        let claim = job.claim.as_ref().context("Unclaimed shared job")?;
        execute(&self.db,"UPDATE relay_chat_jobs SET lease_until=0 WHERE workspace=$1 AND reply=$2 AND claim=$3 AND state='running'",vec![job.workspace.clone().into(),job.reply.clone().into(),claim.clone().into()]).await?;
        Ok(())
    }
}

use super::*;
use crate::identity::chat::store::Job;
impl PgStore {
    pub async fn claim_job(&self) -> Result<Option<Job>> {
        let tx = self.db.begin().await?;
        // Conversation-only generation may retry. Tool-enabled jobs need action receipts first.
        let exhausted=rows(&tx,"UPDATE relay_chat_jobs SET state='finished' WHERE state='running' AND lease_until<=$1 AND attempts>=3 RETURNING reply",vec![now().into()]).await?;
        for row in exhausted {
            execute(&tx,"UPDATE relay_chat_messages SET state='failed',content='The agent could not recover this request. Send a new request to retry.' WHERE id=$1 AND state='queued'",vec![row.try_get::<String>("","reply")?.into()]).await?;
        }
        let claim = uuid::Uuid::new_v4().to_string();
        let row=query(&tx,"WITH ready AS (SELECT reply FROM relay_chat_jobs WHERE (state='queued' OR (state='running' AND lease_until<=$1)) AND attempts<3 ORDER BY sequence FOR UPDATE SKIP LOCKED LIMIT 1) UPDATE relay_chat_jobs j SET state='running',claim=$2,lease_until=$3,attempts=attempts+1 FROM ready WHERE j.reply=ready.reply RETURNING j.payload",vec![now().into(),claim.clone().into(),(now()+300).into()]).await?;
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
        Ok(query(&self.db,"SELECT reply FROM relay_chat_jobs WHERE workspace=$1 AND reply=$2 AND claim=$3 AND state='running' AND lease_until>$4",vec![job.workspace.clone().into(),job.reply.clone().into(),claim.clone().into(),now().into()]).await?.is_some())
    }
}

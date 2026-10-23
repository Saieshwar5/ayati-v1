//! Controller-authorized, per-attempt output receipts and current draft projection.
use super::{
    worker_control::{CLOCK, controlled, require},
    *,
};
use crate::identity::worker_control::output::Update;
impl PgStore {
    pub(crate) async fn append_output(&self, input: &Update, hash: &str) -> Result<i64> {
        input.validate()?;
        let request = &input.assignment;
        let scope = &request.scope;
        let tx = self.workspace_tx(&scope.workspace).await?;
        let record = controlled(&tx, &scope.assignment, hash).await?;
        require(record.scope == *scope && record.session.as_deref() == Some(&request.session))?;
        require(query(&tx,&format!("SELECT reply FROM relay_chat_jobs WHERE workspace=$1 AND reply=$2 AND claim=$3 AND state='running' AND lease_until>{CLOCK} FOR UPDATE"),vec![scope.workspace.clone().into(),scope.reply.clone().into(),scope.assignment.clone().into()]).await?.is_some())?;
        super::controlled_attempts::access(&tx, &record.assignment.job).await?;
        let fingerprint = super::super::provider::digest(&serde_json::to_string(input)?);
        if let Some(row) = query(
            &tx,
            "SELECT fingerprint FROM relay_assignment_output WHERE assignment=$1 AND sequence=$2",
            vec![scope.assignment.clone().into(), input.sequence.into()],
        )
        .await?
        {
            ensure!(
                row.try_get::<String>("", "fingerprint")? == fingerprint,
                "Output replay payload changed"
            );
            return Ok(input.sequence);
        }
        let capability=query(&tx,"SELECT output_sequence,output_bytes FROM relay_assignment_credentials WHERE assignment=$1 FOR UPDATE",vec![scope.assignment.clone().into()]).await?.context("Missing output capability")?;
        let previous: i64 = capability.try_get("", "output_sequence")?;
        let bytes: i32 = capability.try_get("", "output_bytes")?;
        ensure!(input.sequence == previous + 1, "Output sequence gap");
        ensure!(
            bytes as usize + input.text.len() <= 65536,
            "Reply output exceeds limit"
        );
        let message=query(&tx,"SELECT progress FROM relay_chat_messages WHERE workspace=$1 AND id=$2 AND state='queued' FOR UPDATE",vec![scope.workspace.clone().into(),scope.reply.clone().into()]).await?.context("Reply is no longer pending")?;
        let progress: Option<Value> = message.try_get("", "progress")?;
        let text = format!(
            "{}{}",
            progress
                .as_ref()
                .and_then(|v| v["text"].as_str())
                .unwrap_or(""),
            input.text
        );
        execute(&tx,"INSERT INTO relay_assignment_output(assignment,sequence,fingerprint,phase,text) VALUES($1,$2,$3,$4,$5)",vec![scope.assignment.clone().into(),input.sequence.into(),fingerprint.into(),input.phase.clone().into(),input.text.clone().into()]).await?;
        execute(&tx,"UPDATE relay_assignment_credentials SET output_sequence=$2,output_bytes=$3 WHERE assignment=$1",vec![scope.assignment.clone().into(),input.sequence.into(),(bytes+input.text.len() as i32).into()]).await?;
        execute(&tx,"UPDATE relay_worker_assignments SET prompt_reported_ms=COALESCE(prompt_reported_ms,(extract(epoch FROM clock_timestamp())*1000)::bigint),first_output_ms=CASE WHEN $2 THEN COALESCE(first_output_ms,(extract(epoch FROM clock_timestamp())*1000)::bigint) ELSE first_output_ms END WHERE id=$1",vec![scope.assignment.clone().into(),(!input.text.is_empty()).into()]).await?;
        execute(
            &tx,
            "UPDATE relay_chat_messages SET progress=$3 WHERE workspace=$1 AND id=$2",
            vec![
                scope.workspace.clone().into(),
                scope.reply.clone().into(),
                serde_json::json!({"state":input.phase,"text":text,"generation":scope.generation})
                    .into(),
            ],
        )
        .await?;
        tx.commit().await?;
        Ok(input.sequence)
    }
}

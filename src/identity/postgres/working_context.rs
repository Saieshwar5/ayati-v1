//! Bounded, attributed summaries. These are model interpretations, not controller state.
use super::*;
use crate::identity::chat::store::Job;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeSet;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Entry {
    text: String,
    sources: Vec<String>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Brief {
    revision: i64,
    objective: Entry,
    decisions: Vec<Entry>,
    questions: Vec<Entry>,
    corrections: Vec<Entry>,
}
impl Brief {
    fn validate(&self, eligible: &BTreeSet<String>) -> Result<()> {
        ensure!(
            self.revision >= 0
                && self.decisions.len() <= 8
                && self.questions.len() <= 8
                && self.corrections.len() <= 8,
            "Summary exceeds supported bounds"
        );
        for entry in std::iter::once(&self.objective)
            .chain(&self.decisions)
            .chain(&self.questions)
            .chain(&self.corrections)
        {
            ensure!(
                !entry.text.trim().is_empty()
                    && entry.text.len() <= 800
                    && !entry.sources.is_empty()
                    && entry.sources.len() <= 8,
                "Each summary entry needs bounded text and evidence IDs"
            );
            ensure!(
                entry.sources.iter().all(|id| eligible.contains(id)),
                "Summary cites evidence outside this attempt"
            );
        }
        ensure!(
            serde_json::to_vec(self)?.len() <= 12000,
            "Summary exceeds 12,000 bytes"
        );
        Ok(())
    }
}
fn collect_ids(value: &Value, ids: &mut BTreeSet<String>) {
    if let Some(id) = value.get("id").and_then(Value::as_str) {
        ids.insert(id.to_owned());
    }
    if let Some(values) = value.as_array() {
        for v in values {
            collect_ids(v, ids);
        }
    }
}
pub(super) async fn latest(tx: &DatabaseTransaction, job: &Job) -> Result<Value> {
    if !super::context_attempts::exists(tx).await? {
        return Ok(json!({"revision":0,"available":false}));
    }
    let row=query(tx,"SELECT revision,generation,brief,cutoff,task,claim FROM relay_working_context WHERE workspace=$1 AND agent=$2 AND chat=$3 AND requester=$4 ORDER BY revision DESC LIMIT 1",vec![job.workspace.clone().into(),job.agent.clone().into(),job.chat.clone().into(),job.requester.clone().into()]).await?;
    let Some(row) = row else {
        return Ok(json!({"revision":0,"available":false}));
    };
    let revision: i64 = row.try_get("", "revision")?;
    let available = row.try_get::<Value>("", "generation")?
        == super::context_attempts::generation(tx, job).await?;
    let mut result = json!({"revision":revision,"available":available});
    if available {
        result["brief"] = row.try_get("", "brief")?;
        result["cutoff"] = json!(row.try_get::<i64>("", "cutoff")?);
        result["task"] = json!(row.try_get::<String>("", "task")?);
    }
    Ok(result)
}
impl PgStore {
    pub(crate) async fn working_context(&self, job: &Job) -> Result<Value> {
        let tx = self.read_tx(&job.workspace).await?;
        super::control_actions::authorize(&tx, job, true).await?;
        let mut result = latest(&tx, job).await?;
        let sequence = query(
            &tx,
            "SELECT sequence FROM relay_chat_messages WHERE workspace=$1 AND id=$2",
            vec![job.workspace.clone().into(), job.reply.clone().into()],
        )
        .await?
        .context("Task missing")?
        .try_get::<i64>("", "sequence")?;
        // A later concurrent task must never send future context back into an older task.
        if result["cutoff"]
            .as_i64()
            .is_some_and(|cutoff| cutoff >= sequence)
        {
            result = json!({"revision":result["revision"],"available":false});
        }
        tx.commit().await?;
        Ok(result)
    }
    pub(crate) async fn save_working_context(&self, job: &Job, input: &Brief) -> Result<Value> {
        let tx = self.workspace_tx(&job.workspace).await?;
        super::control_actions::authorize(&tx, job, true).await?;
        let row=query(&tx,"SELECT evidence,generation,prepared FROM relay_context_attempts WHERE workspace=$1 AND task=$2 AND claim=$3",vec![job.workspace.clone().into(),job.reply.clone().into(),job.claim.clone().unwrap_or_default().into()]).await?.context("No saved attempt context")?;
        let evidence: Value = row.try_get("", "evidence")?;
        let prepared: Value = row.try_get("", "prepared")?;
        // External-source context retains its existing source-aware paths. Do not turn it
        // into generic conversation knowledge with weaker permissions.
        ensure!(
            prepared["sources"].as_array().is_some_and(Vec::is_empty),
            "External-source tasks cannot save conversation summaries"
        );
        ensure!(!query(&tx,"SELECT EXISTS(SELECT 1 FROM relay_control_actions WHERE workspace=$1 AND job=$2) OR EXISTS(SELECT 1 FROM relay_agent_handoffs WHERE workspace=$1 AND (parent=$2 OR child=$2 OR root=$2)) OR EXISTS(SELECT 1 FROM relay_task_questions WHERE workspace=$1 AND reply=$2) AS derived",vec![job.workspace.clone().into(),job.reply.clone().into()]).await?.unwrap().try_get::<bool>("","derived")?,"Use source-aware task records for operations, questions and handoffs");
        let previous = latest(&tx, job).await?;
        let revision = previous["revision"].as_i64().unwrap_or(0);
        let body = serde_json::to_value(input)?;
        if previous["available"] == true
            && previous["task"] == job.reply
            && previous["brief"] == body
        {
            return Ok(json!({"state":"saved","revision":revision}));
        }
        ensure!(
            input.revision == revision,
            "Working context changed; use the next task's fresh revision"
        );
        let cutoff = query(
            &tx,
            "SELECT sequence FROM relay_chat_messages WHERE workspace=$1 AND id=$2",
            vec![job.workspace.clone().into(), job.reply.clone().into()],
        )
        .await?
        .context("Task missing")?
        .try_get::<i64>("", "sequence")?;
        ensure!(
            previous["cutoff"].as_i64().is_none_or(|v| v <= cutoff),
            "A newer task already summarized this conversation"
        );
        let mut eligible = BTreeSet::new();
        collect_ids(&evidence["conversation"]["messages"], &mut eligible);
        collect_ids(&evidence["notes"], &mut eligible);
        let reads=json_rows(&tx,"SELECT result AS data FROM relay_context_reads WHERE workspace=$1 AND task=$2 AND claim=$3",vec![job.workspace.clone().into(),job.reply.clone().into(),job.claim.clone().unwrap_or_default().into()]).await?;
        for read in reads {
            collect_ids(&read["messages"], &mut eligible);
            collect_ids(&read["message"], &mut eligible);
        }
        input.validate(&eligible)?;
        execute(&tx,"INSERT INTO relay_working_context(workspace,agent,chat,requester,revision,task,claim,generation,cutoff,brief) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",vec![job.workspace.clone().into(),job.agent.clone().into(),job.chat.clone().into(),job.requester.clone().into(),(revision+1).into(),job.reply.clone().into(),job.claim.clone().unwrap_or_default().into(),row.try_get::<Value>("","generation")?.into(),cutoff.into(),body.into()]).await?;
        tx.commit().await?;
        Ok(json!({"state":"saved","revision":revision+1}))
    }
}

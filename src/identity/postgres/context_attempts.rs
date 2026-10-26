//! Protected attempt evidence. Replays never confer access to an old source.
use super::*;
use crate::identity::chat::{execution::Prepared, store::Job};
use serde_json::json;

pub(super) async fn exists(db: &impl ConnectionTrait) -> Result<bool> {
    Ok(query(
        db,
        "SELECT to_regclass('relay_context_attempts') IS NOT NULL AS present",
        vec![],
    )
    .await?
    .context("Context schema check missing")?
    .try_get("", "present")?)
}
pub(super) async fn generation(db: &impl ConnectionTrait, job: &Job) -> Result<Value> {
    Ok(json!(json_rows(db,
        "SELECT jsonb_build_array(category,agent,chat,revision) AS data FROM relay_context_epochs WHERE workspace=$1 AND (agent='' OR agent=$2) AND (chat='' OR chat=$3) ORDER BY category,agent,chat",
        vec![job.workspace.clone().into(),job.agent.clone().into(),job.chat.clone().into()]).await?))
}
pub(super) async fn allowed(db: &impl ConnectionTrait, job: &Job) -> Result<bool> {
    valid(db, job, false).await
}
// Independent artifacts already validate exact connector/library provenance.
// Only content mutations (notes/profile/messages) apply in addition; current
// membership is checked by the artifact reader, independently of prior revocation.
pub(super) async fn knowledge_allowed(db: &impl ConnectionTrait, job: &Job) -> Result<bool> {
    valid(db, job, true).await
}
async fn valid(db: &impl ConnectionTrait, job: &Job, local_only: bool) -> Result<bool> {
    if !exists(db).await? {
        return Ok(true);
    }
    let Some(row) = query(
        db,
        "SELECT generation FROM relay_context_attempts WHERE workspace=$1 AND task=$2 AND claim=COALESCE($3,(SELECT claim FROM relay_chat_jobs WHERE workspace=$1 AND reply=$2))",
        vec![
            job.workspace.clone().into(),
            job.reply.clone().into(),
            job.claim.clone().into(),
        ],
    )
    .await?
    else {
        return Ok(true);
    };
    let mut saved: Value = row.try_get("", "generation")?;
    let mut current = generation(db, job).await?;
    if local_only {
        for value in [&mut saved, &mut current] {
            value
                .as_array_mut()
                .context("Invalid context generation")?
                .retain(|v| v[0] == "context");
        }
    }
    Ok(saved == current)
}

pub(super) fn evidence(prepared: &Prepared) -> Result<Value> {
    let messages = if let Some((_, tail)) = prepared
        .request
        .split_once("Conversation evidence (JSON):\n")
    {
        let mut stream = serde_json::Deserializer::from_str(tail).into_iter::<Value>();
        stream.next().transpose()?.unwrap_or(Value::Null)
    } else {
        Value::Null
    };
    let notes = prepared
        .reference_notes
        .lines()
        .map(serde_json::from_str::<Value>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(json!({"conversation":messages,"notes":notes}))
}

impl PgStore {
    pub(crate) async fn context_generation(&self, job: &Job) -> Result<Option<Value>> {
        let tx = self.read_tx(&job.workspace).await?;
        let result = if exists(&tx).await? {
            Some(generation(&tx, job).await?)
        } else {
            None
        };
        tx.commit().await?;
        Ok(result)
    }
    pub(crate) async fn save_context_attempt(
        &self,
        job: &Job,
        expected: &Value,
        prepared: &Prepared,
        working: &Value,
    ) -> Result<()> {
        let tx = self.workspace_tx(&job.workspace).await?;
        super::control_actions::authorize(&tx, job, true).await?;
        ensure!(
            &generation(&tx, job).await? == expected,
            "Context sources changed during assembly; retry with fresh context"
        );
        let claim = job
            .claim
            .clone()
            .context("Context requires an execution claim")?;
        let mut selected = evidence(prepared)?;
        selected["working"] = working.clone();
        selected["profile_revision"] = json!(
            query(
                &tx,
                "SELECT revision FROM relay_shared_agents WHERE workspace=$1 AND id=$2",
                vec![job.workspace.clone().into(), job.agent.clone().into()]
            )
            .await?
            .context("Agent profile missing")?
            .try_get::<i64>("", "revision")?
        );
        // Exact start replay retains the first delivered context rather than rewriting history.
        execute(&tx,"INSERT INTO relay_context_attempts(workspace,task,claim,agent,chat,requester,generation,prepared,evidence) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT(workspace,task,claim) DO NOTHING",vec![job.workspace.clone().into(),job.reply.clone().into(),claim.into(),job.agent.clone().into(),job.chat.clone().into(),job.requester.clone().into(),expected.clone().into(),serde_json::to_value(prepared)?.into(),selected.into()]).await?;
        tx.commit().await?;
        Ok(())
    }
    pub(crate) async fn recorded_context(&self, job: &Job) -> Result<Option<Prepared>> {
        let tx = self.read_tx(&job.workspace).await?;
        if !exists(&tx).await? {
            return Ok(None);
        }
        super::control_actions::authorize(&tx, job, true).await?;
        let row=query(&tx,"SELECT prepared FROM relay_context_attempts WHERE workspace=$1 AND task=$2 AND claim=$3",vec![job.workspace.clone().into(),job.reply.clone().into(),job.claim.clone().unwrap_or_default().into()]).await?;
        let result = row
            .map(|r| -> Result<Prepared> {
                Ok(serde_json::from_value(r.try_get("", "prepared")?)?)
            })
            .transpose()?;
        tx.commit().await?;
        Ok(result)
    }
}

// Called in the same transaction as the bounded history read, before returning it to the model.
pub(super) async fn record_read(
    tx: &DatabaseTransaction,
    job: &Job,
    tool: &str,
    result: &Value,
) -> Result<()> {
    if !exists(tx).await? {
        return Ok(());
    }
    let claim = job.claim.clone().unwrap_or_default();
    if query(
        tx,
        "SELECT task FROM relay_context_attempts WHERE workspace=$1 AND task=$2 AND claim=$3",
        vec![
            job.workspace.clone().into(),
            job.reply.clone().into(),
            claim.clone().into(),
        ],
    )
    .await?
    .is_none()
    {
        return Ok(());
    }
    ensure!(allowed(tx, job).await?, "Context sources changed");
    let used=query(tx,"SELECT COALESCE(sum(octet_length(result::text)),0)::bigint AS bytes FROM relay_context_reads WHERE workspace=$1 AND task=$2 AND claim=$3",vec![job.workspace.clone().into(),job.reply.clone().into(),claim.clone().into()]).await?.context("History receipt budget unavailable")?.try_get::<i64>("","bytes")?;
    ensure!(
        used + serde_json::to_vec(result)?.len() as i64 <= 262144,
        "This attempt reached its recorded history budget"
    );
    execute(tx,"INSERT INTO relay_context_reads(workspace,task,claim,sequence,tool,result) SELECT $1,$2,$3,COALESCE(max(sequence),0)+1,$4,$5 FROM relay_context_reads WHERE workspace=$1 AND task=$2 AND claim=$3",vec![job.workspace.clone().into(),job.reply.clone().into(),claim.into(),tool.into(),result.clone().into()]).await?;
    Ok(())
}

pub(super) async fn inspect(
    tx: &DatabaseTransaction,
    user: &User,
    w: &str,
    agent: &str,
    chat: &str,
    detail: Option<(&str, &str)>,
) -> Result<Value> {
    if !exists(tx).await? {
        return Ok(json!([]));
    }
    let attempts=json_rows(tx,"SELECT jsonb_build_object('task',task,'claim',claim,'created_at',created_at,'generation',generation,'evidence',CASE WHEN $5::text IS NOT NULL THEN evidence END,'model',prepared->'model','source_count',jsonb_array_length(prepared->'sources')) AS data FROM relay_context_attempts WHERE workspace=$1 AND agent=$2 AND chat=$3 AND requester=$4 AND ($5::text IS NULL OR (task=$5 AND claim=$6)) ORDER BY created_at DESC,task,claim LIMIT 20",vec![w.into(),agent.into(),chat.into(),user.id.clone().into(),detail.map(|v|v.0).into(),detail.map(|v|v.1).into()]).await?;
    let job = Job {
        workspace: w.into(),
        agent: agent.into(),
        chat: chat.into(),
        requester: user.id.clone(),
        reply: String::new(),
        prompt: String::new(),
        claim: None,
    };
    let current = generation(tx, &job).await?;
    let mut visible = vec![];
    for mut attempt in attempts {
        let mut original = job.clone();
        original.reply = attempt["task"].as_str().context("Missing task ID")?.into();
        original.claim = Some(
            attempt["claim"]
                .as_str()
                .context("Missing attempt ID")?
                .into(),
        );
        let available = attempt["generation"] == current
            && super::source_access::allowed(tx, &original).await?;
        attempt.as_object_mut().unwrap().remove("generation");
        attempt["available"] = json!(available);
        if available && detail.is_some() {
            attempt["reads"]=json!(json_rows(tx,"SELECT jsonb_build_object('sequence',sequence,'tool',tool,'result',result,'at',created_at) AS data FROM relay_context_reads WHERE workspace=$1 AND task=$2 AND claim=$3 ORDER BY sequence",vec![w.into(),attempt["task"].as_str().unwrap().into(),attempt["claim"].as_str().unwrap().into()]).await?);
        } else {
            attempt.as_object_mut().unwrap().remove("evidence");
            attempt.as_object_mut().unwrap().remove("model");
            attempt.as_object_mut().unwrap().remove("source_count");
        }
        visible.push(attempt);
    }
    Ok(json!(visible))
}

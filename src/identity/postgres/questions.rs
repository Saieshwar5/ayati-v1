//! Ordinary facts and their designated respondent remain separate from approvals.
use super::*;
use crate::identity::{
    chat::store::Job,
    interactions::model::{Ask, Specification, Submission},
};
use serde_json::json;

impl PgStore {
    pub(crate) async fn ask_question(&self, job: &Job, input: &Ask) -> Result<Value> {
        input.validate()?;
        let hash = fingerprint(serde_json::to_value(input)?)?;
        let tx = self.workspace_tx(&job.workspace).await?;
        super::control_actions::authorize(&tx, job, true).await?;
        if let Some(row) = query(&tx, "SELECT to_jsonb(q) AS data FROM relay_task_questions q WHERE workspace=$1 AND reply=$2 AND request_key=$3", vec![job.workspace.clone().into(),job.reply.clone().into(),input.key.clone().into()]).await? {
            let data:Value = row.try_get("", "data")?;
            ensure!(data["request_hash"] == hash, "Question key already refers to different content; use a new key for a follow-up");
            let result = tool_view(&data);
            tx.commit().await?;
            return Ok(result);
        }
        let row = query(&tx,"SELECT j.run_cycle,(SELECT count(*) FROM relay_task_questions q WHERE q.workspace=j.workspace AND q.reply=j.reply) AS count,(SELECT count(*) FROM relay_task_questions q WHERE q.workspace=j.workspace AND q.reply=j.reply AND q.state='pending') AS pending FROM relay_chat_jobs j WHERE j.workspace=$1 AND j.reply=$2",vec![job.workspace.clone().into(),job.reply.clone().into()]).await?.context("Task unavailable")?;
        ensure!(
            row.try_get::<i64>("", "pending")? == 0,
            "An earlier question still needs an answer; group related fields in one form"
        );
        ensure!(
            row.try_get::<i64>("", "count")? < 10 && row.try_get::<i32>("", "run_cycle")? < 20,
            "Task reached its supported question/continuation limit"
        );
        let id = uuid::Uuid::new_v4().to_string();
        execute(&tx,"INSERT INTO relay_task_questions(workspace,id,reply,request_key,request_hash,run_cycle,specification) VALUES($1,$2,$3,$4,$5,$6,$7)",vec![job.workspace.clone().into(),id.clone().into(),job.reply.clone().into(),input.key.clone().into(),hash.into(),row.try_get::<i32>("","run_cycle")?.into(),serde_json::to_value(&input.specification)?.into()]).await?;
        audit(
            &tx,
            &job.requester,
            Some(&job.workspace),
            "agent_question_created",
            &id,
        )
        .await?;
        tx.commit().await?;
        Ok(
            json!({"id":id,"key":input.key,"revision":1,"state":"pending","message":"Question saved for the original requester. Explain what is waiting, then finish this attempt without polling or more business operations. The original task will continue after an answer or decline."}),
        )
    }
    pub(crate) async fn question(&self, user: &User, w: &str, id: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        let (data, job) = record(&tx, w, id).await?;
        readable(&tx, user, &job).await?;
        let mut result = tool_view(&data);
        result["specification"] = data["specification"].clone();
        result["respondent"] = json!(job.requester);
        result["respondent_name"] = data["respondent_name"].clone();
        result["can_respond"] = json!(
            data["state"] == "pending"
                && job.requester == user.id
                && super::control_actions::authorize(&tx, &job, false)
                    .await
                    .is_ok()
        );
        result["reply"] = json!(job.reply);
        result["chat"] = json!(job.chat);
        result["answered_by"] = data["answered_by"].clone();
        result["answered_ms"] = data["answered_ms"].clone();
        tx.commit().await?;
        Ok(result)
    }
    pub(crate) async fn answer_question(
        &self,
        user: &User,
        w: &str,
        id: &str,
        input: &Submission,
    ) -> Result<Value> {
        uuid::Uuid::parse_str(&input.id).context("Invalid submission identity")?;
        ensure!(
            input.revision == 1 && ["answer", "decline"].contains(&input.decision.as_str()),
            "Unsupported question revision or response"
        );
        let tx = self.workspace_tx(w).await?;
        let (data, job) = record(&tx, w, id).await?;
        readable(&tx, user, &job).await?;
        ensure!(
            job.requester == user.id,
            "Only the intended requester can answer this question"
        );
        ensure!(
            data["revision"] == input.revision,
            "Question changed; reload before answering"
        );
        let spec: Specification = serde_json::from_value(data["specification"].clone())?;
        spec.validate()?;
        let answer = if input.decision == "answer" {
            spec.answer(&input.values)?
        } else {
            ensure!(
                input.values == json!({}),
                "Declining does not submit field values"
            );
            json!({})
        };
        let hash = fingerprint(
            json!({"revision":input.revision,"decision":input.decision,"values":answer}),
        )?;
        if data["submission"] == input.id {
            ensure!(
                data["submission_hash"] == hash,
                "Submission identity already refers to different values"
            );
            let result = tool_view(&data);
            tx.commit().await?;
            return Ok(result);
        }
        ensure!(
            data["state"] == "pending",
            "This question is no longer waiting for an answer"
        );
        super::control_actions::authorize(&tx, &job, false).await?;
        ensure!(query(&tx,"SELECT reply FROM relay_chat_jobs WHERE workspace=$1 AND reply=$2 AND state<>'finished'",vec![w.into(),job.reply.clone().into()]).await?.is_some(), "Task is no longer active");
        let state = if input.decision == "answer" {
            "answered"
        } else {
            "declined"
        };
        execute(&tx,"UPDATE relay_task_questions SET state=$3,answer=$4,submission=$5,submission_hash=$6,answered_by=$7,answered_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint WHERE workspace=$1 AND id=$2 AND state='pending'",vec![w.into(),id.into(),state.into(),answer.into(),input.id.clone().into(),hash.into(),user.id.clone().into()]).await?;
        audit(
            &tx,
            &user.id,
            Some(w),
            if state == "answered" {
                "agent_question_answered"
            } else {
                "agent_question_declined"
            },
            id,
        )
        .await?;
        let (saved, _) = record(&tx, w, id).await?;
        let result = tool_view(&saved);
        tx.commit().await?;
        Ok(result)
    }
}
pub(super) fn tool_view(data: &Value) -> Value {
    json!({"id":data["id"],"key":data["request_key"],"revision":data["revision"],"state":data["state"],"values":data["answer"]})
}
fn fingerprint(mut value: Value) -> Result<String> {
    // JSON object order is not semantic and PostgreSQL JSONB normalizes it.
    value.sort_all_objects();
    Ok(super::super::provider::digest(&serde_json::to_string(
        &value,
    )?))
}
async fn record(tx: &DatabaseTransaction, w: &str, id: &str) -> Result<(Value, Job)> {
    let row=query(tx,"SELECT to_jsonb(q)||jsonb_build_object('respondent_name',COALESCE(u.display_name,u.email,'Former member')) AS data,j.payload FROM relay_task_questions q JOIN relay_chat_jobs j ON j.workspace=q.workspace AND j.reply=q.reply LEFT JOIN relay_users u ON u.id=j.payload->>'requester' WHERE q.workspace=$1 AND q.id=$2",vec![w.into(),id.into()]).await?.context("Question unavailable")?;
    let data: Value = row.try_get("", "data")?;
    let job: Job = serde_json::from_value(row.try_get("", "payload")?)?;
    ensure!(
        job.workspace == w && data["reply"] == job.reply,
        "Question scope mismatch"
    );
    Ok((data, job))
}
pub(super) async fn readable(tx: &DatabaseTransaction, user: &User, job: &Job) -> Result<()> {
    super::chat::access(tx, user, &job.workspace, &job.chat).await?;
    let mut viewer = job.clone();
    viewer.requester = user.id.clone();
    ensure!(
        super::library::source_allowed(tx, &viewer).await?,
        "Question source unavailable"
    );
    Ok(())
}

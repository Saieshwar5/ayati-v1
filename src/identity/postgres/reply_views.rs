//! Draft reads use the same current requester/source authority as output writes.
use super::*;
use crate::identity::chat::store::Job;
pub(super) async fn hydrate(
    tx: &DatabaseTransaction,
    user: &User,
    workspace: &str,
    messages: &mut [Value],
) -> Result<()> {
    super::approval_delivery::hydrate(tx, user, workspace, messages).await?;
    super::question_views::hydrate(tx, user, workspace, messages).await?;
    let ids = messages
        .iter()
        .filter(|m| m["kind"] == "agent")
        .map(|m| m["id"].clone())
        .collect::<Vec<_>>();
    if ids.is_empty() {
        return Ok(());
    }
    let owner = role(tx, workspace, &user.id).await? == "Owner";
    let mut jobs = std::collections::HashMap::new();
    let pending = messages
        .iter()
        .filter(|m| m["kind"] == "agent" && m["state"] == "queued")
        .map(|m| m["id"].clone())
        .collect::<Vec<_>>();
    let event_clause = if super::event_access::exists(tx).await? {
        " OR EXISTS(SELECT 1 FROM relay_event_deliveries e WHERE e.workspace=j.workspace AND e.reply=j.reply)"
    } else {
        ""
    };
    let sql = format!(
        "SELECT j.payload FROM relay_chat_jobs j WHERE j.workspace=$1 AND j.reply IN (SELECT jsonb_array_elements_text($2::jsonb)) AND (j.reply IN (SELECT jsonb_array_elements_text($3::jsonb)) OR EXISTS(SELECT 1 FROM relay_chat_messages interrupted WHERE interrupted.workspace=j.workspace AND interrupted.id=j.reply AND interrupted.state NOT IN ('queued','complete','sent')) OR EXISTS(SELECT 1 FROM relay_library_sources s WHERE s.workspace=j.workspace AND s.reply=j.reply){event_clause})"
    );
    for row in rows(
        tx,
        &sql,
        vec![
            workspace.into(),
            serde_json::json!(ids).into(),
            serde_json::json!(pending).into(),
        ],
    )
    .await?
    {
        let job: Job = serde_json::from_value(row.try_get("", "payload")?)?;
        jobs.insert(job.reply.clone(), job);
    }
    for message in messages.iter_mut().filter(|m| m["kind"] == "agent") {
        if let Some(job) = message["id"].as_str().and_then(|id| jobs.get(id)) {
            message["source_scope"] =
                serde_json::json!(super::event_access::scope(tx, workspace, &job.reply).await?);
            let mut viewer = job.clone();
            viewer.requester = user.id.clone();
            if !super::source_access::allowed(tx, &viewer).await? {
                message["content"] =
                    Value::String("Response unavailable because source access changed.".into());
                message["response"] = Value::Null;
                message["progress"] = Value::Null;
                message["attachments"] = serde_json::json!([]);
                message["questions"] = serde_json::json!([]);
                message["can_cancel"] = Value::Bool(false);
                message["response_unavailable"] = Value::Bool(true);
                continue;
            }
            if !matches!(
                message["state"].as_str(),
                Some("queued" | "complete" | "sent")
            ) && let Some(text) =
                super::responses::partial_output(tx, workspace, &job.reply).await?
            {
                let mut reference = crate::response::reference(
                    &job.reply,
                    &text,
                    "interrupted",
                    message["revision"].as_i64().unwrap_or(0),
                );
                reference["long"] = Value::Bool(true);
                message["response"] = reference;
            }
            if message["state"] != "queued" {
                continue;
            }
            message["can_cancel"] = Value::Bool(owner || job.requester == user.id);
            if message["progress"].is_object()
                && super::controlled_attempts::access(tx, job).await.is_err()
            {
                message["progress"] = Value::Null;
                message["content"] =
                    Value::String("Reply unavailable because access changed.".into());
                message["state"] = Value::String("cancelled".into());
                message["can_cancel"] = Value::Bool(false);
            }
        }
    }
    Ok(())
}

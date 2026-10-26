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
        .filter(|m| m["kind"] == "agent" && m["state"] == "queued")
        .map(|m| m["id"].clone())
        .collect::<Vec<_>>();
    if ids.is_empty() {
        return Ok(());
    }
    let owner = role(tx, workspace, &user.id).await? == "Owner";
    let mut jobs = std::collections::HashMap::new();
    for row in rows(tx,"SELECT payload FROM relay_chat_jobs WHERE workspace=$1 AND reply IN (SELECT jsonb_array_elements_text($2::jsonb))",vec![workspace.into(),serde_json::json!(ids).into()]).await? {
        let job:Job=serde_json::from_value(row.try_get("","payload")?)?;
        jobs.insert(job.reply.clone(),job);
    }
    for message in messages
        .iter_mut()
        .filter(|m| m["kind"] == "agent" && m["state"] == "queued")
    {
        if let Some(job) = message["id"].as_str().and_then(|id| jobs.get(id)) {
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

//! Draft reads use the same current requester/source authority as output writes.
use super::*;
use crate::identity::chat::model::Job;
pub(super) async fn hydrate(
    tx: &DatabaseTransaction,
    user: &User,
    workspace: &str,
    messages: &mut [Value],
) -> Result<()> {
    super::handoff_views::hydrate(tx, user, workspace, messages).await?;
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
    let task_ids = rows(tx,"SELECT reply FROM relay_chat_jobs WHERE workspace=$1 AND reply IN (SELECT jsonb_array_elements_text($2::jsonb))",vec![workspace.into(),serde_json::json!(ids).into()]).await?.into_iter().map(|r|r.try_get::<String>("","reply")).collect::<std::result::Result<std::collections::HashSet<_>,_>>()?;
    for message in messages.iter_mut() {
        message["has_task"] = Value::Bool(
            message["id"]
                .as_str()
                .is_some_and(|id| task_ids.contains(id)),
        );
    }
    let owner = role(tx, workspace, &user.id).await? == "Admin";
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
    let handoff_clause = if super::handoff_access::exists(tx).await? {
        " OR EXISTS(SELECT 1 FROM relay_agent_handoffs h WHERE h.workspace=j.workspace AND (h.child=j.reply OR h.root=j.reply))"
    } else {
        ""
    };
    let context_clause = if super::context_attempts::exists(tx).await? {
        " OR EXISTS(SELECT 1 FROM relay_context_attempts ca WHERE ca.workspace=j.workspace AND ca.task=j.reply)"
    } else {
        ""
    };
    let communication_clause = if super::communications::access::exists(tx).await? {
        " OR EXISTS(SELECT 1 FROM relay_communication_inbox ci WHERE ci.workspace=j.workspace AND ci.reply=j.reply AND ci.transport<>'chat')"
    } else {
        ""
    };
    let home_clause = if query(
        tx,
        "SELECT to_regclass('relay_home_update_runs') IS NOT NULL AS ready",
        vec![],
    )
    .await?
    .unwrap()
    .try_get::<bool>("", "ready")?
    {
        " OR EXISTS(SELECT 1 FROM relay_home_update_runs hu WHERE hu.workspace=j.workspace AND hu.reply=j.reply)"
    } else {
        ""
    };
    let sql = format!(
        "SELECT j.payload FROM relay_chat_jobs j WHERE j.workspace=$1 AND j.reply IN (SELECT jsonb_array_elements_text($2::jsonb)) AND (j.reply IN (SELECT jsonb_array_elements_text($3::jsonb)) OR EXISTS(SELECT 1 FROM relay_chat_messages interrupted WHERE interrupted.workspace=j.workspace AND interrupted.id=j.reply AND interrupted.state NOT IN ('queued','complete','sent')) OR EXISTS(SELECT 1 FROM relay_library_sources s WHERE s.workspace=j.workspace AND s.reply=j.reply){event_clause}{handoff_clause}{context_clause}{communication_clause}{home_clause})"
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
            if !super::source_access::allowed(tx, &viewer).await?
                || !super::context_attempts::allowed(tx, &viewer).await?
            {
                message["content"] =
                    Value::String("Response unavailable because source access changed.".into());
                message["response"] = Value::Null;
                message["progress"] = Value::Null;
                message["attachments"] = serde_json::json!([]);
                message["questions"] = serde_json::json!([]);
                message["can_cancel"] = Value::Bool(false);
                message["response_unavailable"] = Value::Bool(true);
                message["has_task"] = Value::Bool(false);
                continue;
            }
            message["coordination"] = super::handoff_views::project(tx, user, job).await?;
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

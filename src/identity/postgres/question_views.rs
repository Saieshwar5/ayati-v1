//! Batch reference projection; field values are retrieved only in an authorized review.
use super::*;
use crate::identity::chat::store::Job;
use serde_json::json;
pub(super) async fn hydrate(
    tx: &DatabaseTransaction,
    user: &User,
    w: &str,
    messages: &mut [Value],
) -> Result<()> {
    let ids = messages
        .iter()
        .filter(|m| m["kind"] == "agent")
        .map(|m| m["id"].clone())
        .collect::<Vec<_>>();
    if ids.is_empty() {
        return Ok(());
    }
    let records=rows(tx,"SELECT q.id,q.reply,q.state,q.specification->>'title' AS title,j.payload,COALESCE(u.display_name,u.email,'Former member') AS respondent_name FROM relay_task_questions q JOIN relay_chat_jobs j ON j.workspace=q.workspace AND j.reply=q.reply LEFT JOIN relay_users u ON u.id=j.payload->>'requester' WHERE q.workspace=$1 AND q.reply IN (SELECT jsonb_array_elements_text($2::jsonb)) ORDER BY q.created_ms,q.id",vec![w.into(),json!(ids).into()]).await?;
    let mut allowed = std::collections::HashMap::new();
    let mut references = std::collections::HashMap::<String, Vec<Value>>::new();
    for row in records {
        let job: Job = serde_json::from_value(row.try_get("", "payload")?)?;
        let visible = if let Some(visible) = allowed.get(&job.reply) {
            *visible
        } else {
            let visible = super::questions::readable(tx, user, &job).await.is_ok();
            allowed.insert(job.reply.clone(), visible);
            visible
        };
        if !visible {
            continue;
        }
        references.entry(job.reply).or_default().push(json!({"id":row.try_get::<String>("","id")?,"title":row.try_get::<String>("","title")?,"state":row.try_get::<String>("","state")?,"respondent":job.requester,"respondent_name":row.try_get::<String>("","respondent_name")?}));
    }
    for message in messages {
        if let Some(questions) = message["id"].as_str().and_then(|id| references.remove(id)) {
            message["questions"] = json!(questions);
        }
    }
    Ok(())
}

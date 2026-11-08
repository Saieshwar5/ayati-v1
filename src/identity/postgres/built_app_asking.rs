//! "Ask first" for app actions (Apps AP10). When the builder or an admin sets an action
//! to ask first, the agent gets a question Cuckoo wrote — "May Nia file a leave request
//! in Leave?" with the fields — to put to the person through `ask_question`; the action
//! runs only when that person answered yes to that exact question in this request.
use super::*;
use crate::identity::chat::model::Job;
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};
use serde_json::json;

/// Schema 85: app action rules may ask the person first.
pub(crate) struct BuiltAppAsking;
impl MigrationName for BuiltAppAsking {
    fn name(&self) -> &str {
        "m20261105_000085_built_app_asking"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for BuiltAppAsking {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(
                "ALTER TABLE relay_built_app_agent_rules DROP CONSTRAINT relay_built_app_agent_rules_mode_check;
                 ALTER TABLE relay_built_app_agent_rules ADD CONSTRAINT relay_built_app_agent_rules_mode_check CHECK(mode IN ('allow','ask','block'));",
            )
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Asking first requires a forward migration".into(),
        ))
    }
}

/// The question for this exact run: its key (from the action's exact fields) and the specification the agent passes to `ask_question` unchanged.
pub(super) fn question(
    agent: &str,
    action: &str,
    app: &str,
    arguments: &Value,
) -> Result<(String, Value)> {
    // Questions are already one per key within a request, so the exact fields are enough.
    let digest = crate::identity::provider::digest(&serde_json::to_string(arguments)?);
    let key = format!("app-{}", &digest[..20.min(digest.len())]);
    let fields: Vec<String> = arguments["data"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(k, v)| {
            format!(
                "{k}: {}",
                v.as_str().map_or_else(|| v.to_string(), str::to_owned)
            )
        })
        .collect();
    let specification = json!({
        "version": 1,
        "kind": "single_choice",
        "title": format!("May {agent} {} in {app}?", action.chars().next().map_or(String::new(), |c| c.to_lowercase().collect::<String>() + &action[c.len_utf8()..])),
        "description": if fields.is_empty() { String::new() } else { fields.join(" · ").chars().take(1000).collect::<String>() },
        "fallback": "Reply yes or no.",
        "options": [{"id": "yes", "label": "Yes, go ahead"}, {"id": "no", "label": "No"}],
    });
    Ok((key, specification))
}

/// Whether the person answered this question: none while unasked or waiting, yes only
/// for the exact question Cuckoo wrote, answered by the person the agent works for.
pub(in crate::identity::postgres) async fn answered(
    tx: &DatabaseTransaction,
    job: &Job,
    key: &str,
    specification: &Value,
) -> Result<Option<bool>> {
    let Some(row) = query(tx, "SELECT state,specification,answer,answered_by FROM relay_task_questions WHERE workspace=$1 AND reply=$2 AND request_key=$3", vec![job.workspace.clone().into(), job.reply.clone().into(), key.into()]).await? else {
        return Ok(None);
    };
    let state: String = row.try_get("", "state")?;
    if state == "pending" {
        return Ok(None);
    }
    let asked: Value = row.try_get("", "specification")?;
    let same = asked["title"] == specification["title"]
        && asked["options"] == specification["options"]
        && asked["description"] == specification["description"];
    let answer: Option<Value> = row.try_get("", "answer")?;
    let by: Option<String> = row.try_get("", "answered_by")?;
    Ok(Some(
        state == "answered"
            && same
            && by.as_deref() == Some(job.requester.as_str())
            && answer.is_some_and(|a| a["choices"] == json!(["yes"])),
    ))
}

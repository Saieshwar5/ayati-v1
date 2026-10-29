use super::*;
use crate::identity::{chat::model::Job, events::catalog};
use serde_json::json;
pub(super) async fn exists(db: &impl ConnectionTrait) -> Result<bool> {
    Ok(query(
        db,
        "SELECT to_regclass('relay_agent_handoffs') IS NOT NULL AS present",
        vec![],
    )
    .await?
    .unwrap()
    .try_get("", "present")?)
}
pub(super) async fn root(db: &impl ConnectionTrait, job: &Job) -> Result<String> {
    if !exists(db).await? {
        return Ok(job.reply.clone());
    }
    Ok(query(
        db,
        "SELECT root FROM relay_agent_handoffs WHERE workspace=$1 AND child=$2",
        vec![job.workspace.clone().into(), job.reply.clone().into()],
    )
    .await?
    .map(|r| r.try_get("", "root"))
    .transpose()?
    .unwrap_or_else(|| job.reply.clone()))
}
pub(super) async fn tree(db: &impl ConnectionTrait, job: &Job) -> Result<Vec<Value>> {
    if !exists(db).await? {
        return Ok(vec![]);
    }
    json_rows(db,"SELECT to_jsonb(h) AS data FROM relay_agent_handoffs h WHERE workspace=$1 AND root=$2 ORDER BY depth,created_ms,id",vec![job.workspace.clone().into(),root(db,job).await?.into()]).await
}
pub(super) async fn job(db: &impl ConnectionTrait, w: &str, reply: &str) -> Result<Job> {
    serde_json::from_value(
        query(
            db,
            "SELECT payload FROM relay_chat_jobs WHERE workspace=$1 AND reply=$2",
            vec![w.into(), reply.into()],
        )
        .await?
        .context("Handoff source unavailable")?
        .try_get("", "payload")?,
    )
    .map_err(Into::into)
}
pub(super) async fn source_allowed(db: &impl ConnectionTrait, viewer: &Job) -> Result<bool> {
    let records = tree(db, viewer).await?;
    if records.is_empty() {
        return Ok(true);
    }
    let mut source_job = job(db, &viewer.workspace, records[0]["root"].as_str().unwrap()).await?;
    source_job.requester = viewer.requester.clone();
    if !super::context_attempts::knowledge_allowed(db, &source_job).await?
        || !super::library::source_allowed(db, &source_job).await?
        || !super::event_access::allowed(db, &source_job).await?
    {
        return Ok(false);
    }
    for h in records {
        let child = job(
            db,
            &viewer.workspace,
            h["child"].as_str().context("Missing helper source")?,
        )
        .await?;
        if !super::context_attempts::knowledge_allowed(db, &child).await? {
            return Ok(false);
        }
        if h["source_revoked"] == true {
            return Ok(false);
        }
        if h["source"].is_null() {
            continue;
        }
        let s = &h["source"];
        let current = async {
            let source = catalog::source(
                &viewer.workspace,
                s["source"]
                    .as_str()
                    .context("Missing handoff event source")?,
            )?;
            ensure!(
                source.fingerprint()? == s["binding"],
                "Handoff source identity changed"
            );
            let principal = User {
                id: h["principal"].as_str().unwrap().into(),
                email: String::new(),
                display_name: None,
            };
            let versions = super::event_access::versions(
                db,
                &principal,
                &viewer.workspace,
                h["target"].as_str().unwrap(),
                h["chat"].as_str().unwrap(),
                &source,
            )
            .await?;
            ensure!(
                versions == s["authority"],
                "Helper source authority changed"
            );
            Ok::<_, anyhow::Error>(())
        }
        .await;
        if let Err(e) = current {
            if e.is::<sea_orm::DbErr>() {
                return Err(e);
            }
            return Ok(false);
        }
    }
    Ok(true)
}
pub(super) async fn execution_allowed(db: &impl ConnectionTrait, task: &Job) -> Result<bool> {
    let records = tree(db, task).await?;
    if records.is_empty() {
        return Ok(true);
    }
    let mut parent = task.reply.clone();
    for _ in 0..3 {
        let Some(h) = records.iter().find(|h| h["child"] == parent) else {
            return Ok(true);
        };
        ensure!(
            h["principal"] == task.requester && h["chat"] == task.chat,
            "Handoff task identity changed"
        );
        if h["state"] != "pending" {
            return Ok(false);
        }
        let ancestor = job(db, &task.workspace, h["parent"].as_str().unwrap()).await?;
        let user = User {
            id: task.requester.clone(),
            email: String::new(),
            display_name: None,
        };
        if !super::chat::agent_access(db, &task.requester, &task.workspace, &ancestor.agent).await?
            || super::chat::access(db, &user, &task.workspace, &ancestor.chat)
                .await
                .is_err()
        {
            return Ok(false);
        }
        if query(db,"SELECT j.reply FROM relay_chat_jobs j JOIN relay_chat_messages m ON m.workspace=j.workspace AND m.id=j.reply WHERE j.workspace=$1 AND j.reply=$2 AND j.state<>'finished' AND m.state='queued' AND EXISTS(SELECT 1 FROM relay_chat_agents a WHERE a.workspace=j.workspace AND a.chat=$3 AND a.agent=$4)",vec![task.workspace.clone().into(),ancestor.reply.clone().into(),task.chat.clone().into(),ancestor.agent.into()]).await?.is_none(){return Ok(false)}
        parent = ancestor.reply;
    }
    Ok(parent == root(db, task).await?)
}
pub(super) async fn event_source(
    db: &impl ConnectionTrait,
    task: &Job,
    target: &str,
) -> Result<Option<Value>> {
    let root = root(db, task).await?;
    let row=query(db,"SELECT d.id,t.source,t.binding FROM relay_event_deliveries d JOIN relay_event_triggers t ON t.workspace=d.workspace AND t.id=d.trigger_id WHERE d.workspace=$1 AND d.reply=$2 AND NOT d.revoked",vec![task.workspace.clone().into(),root.into()]).await?;
    let Some(row) = row else { return Ok(None) };
    let source = catalog::source(&task.workspace, &row.try_get::<String>("", "source")?)?;
    ensure!(
        source.fingerprint()? == row.try_get::<String>("", "binding")?,
        "Source identity changed"
    );
    let user = User {
        id: task.requester.clone(),
        email: String::new(),
        display_name: None,
    };
    let authority =
        super::event_access::versions(db, &user, &task.workspace, target, &task.chat, &source)
            .await?;
    Ok(Some(
        json!({"source":source.id,"delivery":row.try_get::<String>("","id")?,"binding":source.fingerprint()?,"connection":source.connection,"operation":source.read_operation,"authority":authority}),
    ))
}

pub(super) async fn message_job(
    db: &impl ConnectionTrait,
    w: &str,
    message: &str,
) -> Result<Option<Job>> {
    if !exists(db).await? {
        return Ok(None);
    }
    let row=query(db,"SELECT j.payload FROM relay_agent_handoffs h JOIN relay_chat_jobs j ON j.workspace=h.workspace AND j.reply=h.child WHERE h.workspace=$1 AND h.request_message=$2",vec![w.into(),message.into()]).await?;
    row.map(|r| serde_json::from_value(r.try_get("", "payload")?).map_err(Into::into))
        .transpose()
}

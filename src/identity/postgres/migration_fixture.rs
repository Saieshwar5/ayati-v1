//! Seed historical schemas without using newer intake or registration SQL.
use super::*;
use crate::identity::chat::store::Job;

pub(super) async fn legacy_executor(
    db: &PgStore,
    registration: &crate::identity::worker_control::model::Registration,
) -> Result<()> {
    execute(
        &db.db,
        "INSERT INTO relay_executors(id,incarnation,token_hash,expires,pooled)
        VALUES($1,$2,$3,floor(extract(epoch FROM clock_timestamp()))::bigint+86400,$4)",
        vec![
            registration.executor.clone().into(),
            registration.incarnation.clone().into(),
            super::super::provider::digest(&registration.token).into(),
            registration.pooled.into(),
        ],
    )
    .await?;
    Ok(())
}

pub(super) async fn legacy_request(
    db: &PgStore,
    user: &User,
    workspace: &str,
    chat: &str,
    agent: &str,
    content: &str,
) -> Result<Option<Job>> {
    let tx = db.db.begin().await?;
    execute(&tx,"INSERT INTO relay_chat_messages(id,workspace,chat,author,kind,content,state,at,target) VALUES($1,$2,$3,$4,'user',$5,'sent',$6,$7)",
        vec![uuid::Uuid::new_v4().to_string().into(),workspace.into(),chat.into(),user.id.clone().into(),content.into(),now().into(),agent.into()]).await?;
    let reply = uuid::Uuid::new_v4().to_string();
    execute(&tx,"INSERT INTO relay_chat_messages(id,workspace,chat,author,kind,content,state,at,requester) VALUES($1,$2,$3,$4,'agent','','queued',$5,$6)",
        vec![reply.clone().into(),workspace.into(),chat.into(),agent.into(),now().into(),user.id.clone().into()]).await?;
    let job = Job {
        reply,
        workspace: workspace.into(),
        chat: chat.into(),
        agent: agent.into(),
        requester: user.id.clone(),
        prompt: content.into(),
        claim: None,
    };
    execute(&tx,"INSERT INTO relay_chat_jobs(workspace,reply,payload,dispatch,run_seconds) VALUES($1,$2,$3,$4,$5)",
        vec![workspace.into(),job.reply.clone().into(),serde_json::to_value(&job)?.into(),if db.assigned {"assigned"} else if db.pooled {"pooled"} else {"polling"}.into(),(db.assignment_seconds as i32).into()]).await?;
    tx.commit().await?;
    Ok(Some(job))
}

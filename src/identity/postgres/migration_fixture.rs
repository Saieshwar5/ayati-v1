//! Seed historical schemas without using newer intake or registration SQL.
use super::*;
use crate::identity::chat::model::Job;

/// The request/token pair a retired assigned worker used to reserve work.
pub(super) struct LegacyReservation {
    pub request: String,
    pub token: String,
}

pub(super) async fn legacy_reservation(
    db: &PgStore,
    executor: &crate::identity::worker_control::model::Registration,
    request: &LegacyReservation,
    lease: i32,
) -> Result<crate::identity::worker_control::model::Scope> {
    use crate::identity::worker_control::model::Scope;
    let tx = db.db.begin().await?;
    let id = uuid::Uuid::new_v4().to_string();
    let row=query(&tx,"UPDATE relay_chat_jobs SET state='running',claim=$1,attempts=attempts+1,lease_until=floor(extract(epoch FROM clock_timestamp()))::bigint+300 WHERE reply=(SELECT reply FROM relay_chat_jobs WHERE state='queued' ORDER BY sequence LIMIT 1) RETURNING payload,attempts",vec![id.clone().into()]).await?.context("No historical request")?;
    let job: Job = serde_json::from_value(row.try_get("", "payload")?)?;
    let scope = Scope {
        executor: executor.executor.clone(),
        incarnation: executor.incarnation.clone(),
        slot: uuid::Uuid::new_v4().to_string(),
        slot_generation: 0,
        assignment: id.clone(),
        workspace: job.workspace.clone(),
        reply: job.reply.clone(),
        agent: job.agent,
        chat: job.chat,
        thread: None,
        generation: row.try_get("", "attempts")?,
    };
    execute(&tx,"INSERT INTO relay_worker_assignments(id,workspace,reply,generation,provider,state,lease_seconds,resource_id) VALUES($1,$2,$3,$4,'local','launching',$5,$6)",vec![id.clone().into(),job.workspace.into(),job.reply.into(),scope.generation.into(),lease.into(),scope.resource().into()]).await?;
    execute(&tx,"INSERT INTO relay_assignment_credentials(assignment,executor,incarnation,slot,request,token_hash,expires) VALUES($1,$2,$3,$4,$5,$6,floor(extract(epoch FROM clock_timestamp()))::bigint+300)",vec![id.into(),executor.executor.clone().into(),executor.incarnation.clone().into(),scope.slot.clone().into(),request.request.clone().into(),super::super::provider::digest(&request.token).into()]).await?;
    tx.commit().await?;
    Ok(scope)
}

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
    let dispatch = if db.pooled { "pooled" } else { "polling" };
    insert_request(db, user, workspace, chat, agent, content, dispatch).await
}

/// A request queued for the retired assigned workers, for upgrade tests.
pub(super) async fn legacy_assigned_request(
    db: &PgStore,
    user: &User,
    workspace: &str,
    chat: &str,
    agent: &str,
    content: &str,
) -> Result<Option<Job>> {
    insert_request(db, user, workspace, chat, agent, content, "assigned").await
}

async fn insert_request(
    db: &PgStore,
    user: &User,
    workspace: &str,
    chat: &str,
    agent: &str,
    content: &str,
    dispatch: &str,
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
        vec![workspace.into(),job.reply.clone().into(),serde_json::to_value(&job)?.into(),dispatch.into(),(db.assignment_seconds as i32).into()]).await?;
    tx.commit().await?;
    Ok(Some(job))
}

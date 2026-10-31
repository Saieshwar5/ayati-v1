//! An agent's working voice. While a request is in progress the agent can post
//! short messages about its own work — "got it, I'll…", a milestone — through
//! `send_update`; and when a request queues behind the agent's other work the
//! controller posts an instant "I'm on another request" reply with no model
//! call. Each such message links to its request (`update_of`) and is never the
//! request's reply, so the reply still carries the result and the receipts.
use super::*;
use crate::identity::chat::model::Job;
use serde::Deserialize;
use serde_json::json;

/// Per request: at most this many updates, and after the first, at least this
/// gap between them, so a chatty model can't flood a conversation.
pub(crate) const MAX_UPDATES: i64 = 12;
pub(crate) const MIN_GAP_SECONDS: i64 = 45;
const MAX_CHARS: usize = 1000;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Update {
    pub text: String,
}

/// Whether this schema stores updates (52+), so current code runs on older ones.
pub(crate) async fn ready(db: &impl ConnectionTrait) -> Result<bool> {
    Ok(query(
        db,
        "SELECT to_regclass('relay_messages_updates') IS NOT NULL AS ready",
        vec![],
    )
    .await?
    .map(|r| r.try_get::<bool>("", "ready"))
    .transpose()?
    .unwrap_or(false))
}

/// Posts one update message from `agent` about `reply`, in `thread` of `chat`.
async fn post(
    tx: &DatabaseTransaction,
    w: &str,
    chat: &str,
    thread: Option<&str>,
    agent: &str,
    reply: &str,
    text: &str,
) -> Result<String> {
    let id = uuid::Uuid::new_v4().to_string();
    execute(tx, "INSERT INTO relay_chat_messages(id,workspace,chat,author,kind,content,state,at,thread,update_of) VALUES($1,$2,$3,$4,'agent',$5,'complete',$6,$7,$8)",
        vec![id.clone().into(), w.into(), chat.into(), agent.into(), text.into(), now().into(), thread.map(str::to_owned).into(), reply.into()]).await?;
    Ok(id)
}

impl PgStore {
    /// The `send_update` tool: a message while the work continues. The first
    /// goes where the request was asked; in a channel or group, later ones go
    /// into the thread under the request so the channel isn't flooded.
    pub(crate) async fn send_update(&self, job: &Job, input: &Update) -> Result<Value> {
        let text = input.text.trim();
        if text.is_empty()
            || text.chars().count() > MAX_CHARS
            || text.chars().any(|c| c.is_control() && c != '\n')
        {
            return Ok(
                json!({"state":"invalid","message":format!("An update is 1 to {MAX_CHARS} characters of plain text.")}),
            );
        }
        let tx = self.workspace_tx(&job.workspace).await?;
        super::control_actions::authorize(&tx, job, true).await?;
        let row = query(&tx, "SELECT m.thread,c.kind,j.request_message,
              (SELECT count(*) FROM relay_chat_messages u WHERE u.workspace=m.workspace AND u.update_of=m.id AND u.author=m.author) AS sent,
              (SELECT max(u.at) FROM relay_chat_messages u WHERE u.workspace=m.workspace AND u.update_of=m.id AND u.author=m.author) AS last_at
            FROM relay_chat_messages m JOIN relay_chats c ON c.workspace=m.workspace AND c.id=m.chat
            JOIN relay_chat_jobs j ON j.workspace=m.workspace AND j.reply=m.id WHERE m.workspace=$1 AND m.id=$2",
            vec![job.workspace.clone().into(), job.reply.clone().into()]).await?.context("Request unavailable")?;
        let sent: i64 = row.try_get("", "sent")?;
        if sent >= MAX_UPDATES {
            tx.commit().await?;
            return Ok(
                json!({"state":"limit","message":"You have sent the most updates this request allows. Finish with your reply."}),
            );
        }
        if let Some(last) = row.try_get::<Option<i64>>("", "last_at")?
            && now() - last < MIN_GAP_SECONDS
        {
            tx.commit().await?;
            return Ok(
                json!({"state":"too_soon","message":format!("Send the next update when something new has happened, at least {MIN_GAP_SECONDS} seconds after the last one. Keep working meanwhile.")}),
            );
        }
        let reply_thread: Option<String> = row.try_get("", "thread")?;
        let kind: String = row.try_get("", "kind")?;
        let request: Option<String> = row.try_get("", "request_message")?;
        let thread = match (&reply_thread, kind.as_str(), sent) {
            (Some(thread), _, _) => Some(thread.clone()),
            (None, "direct", _) | (None, _, 0) => None,
            (None, _, _) => request,
        };
        let id = post(
            &tx,
            &job.workspace,
            &job.chat,
            thread.as_deref(),
            &job.agent,
            &job.reply,
            text,
        )
        .await?;
        tx.commit().await?;
        Ok(json!({"state":"sent","message_id":id,"in_thread":thread.is_some()}))
    }
}

/// When a new request queues behind the agent's other work, say so at once, in
/// the agent's voice, without a model call. Names the other person only when
/// the requester can see that conversation. Nothing is posted when the agent
/// is free, or for background work (schedules, triggers, hand-offs).
pub(crate) async fn busy_reply(
    tx: &DatabaseTransaction,
    w: &str,
    job: &Job,
    thread: Option<&str>,
) -> Result<()> {
    if !ready(tx).await? {
        return Ok(());
    }
    let others = rows(tx, "SELECT j.reply,j.payload->>'requester' AS requester,m.chat,
          (j.state='running' AND j.lease_until>floor(extract(epoch FROM clock_timestamp()))::bigint) AS running
        FROM relay_chat_jobs j JOIN relay_chat_messages m ON m.workspace=j.workspace AND m.id=j.reply
        WHERE j.workspace=$1 AND j.payload->>'agent'=$2 AND j.reply<>$3
        AND (j.state='queued' OR (j.state='running' AND j.lease_until>floor(extract(epoch FROM clock_timestamp()))::bigint))
        ORDER BY (j.state='running') DESC, j.sequence", vec![w.into(), job.agent.clone().into(), job.reply.clone().into()]).await?;
    if others.is_empty() {
        return Ok(());
    }
    let current = &others[0];
    let requester: Option<String> = current.try_get("", "requester")?;
    let chat: String = current.try_get("", "chat")?;
    let viewer = User {
        id: job.requester.clone(),
        email: String::new(),
        display_name: None,
    };
    let doing = if requester.as_deref() == Some(job.requester.as_str()) {
        "I'm still working on your earlier request".to_string()
    } else if super::chat::access(tx, &viewer, w, &chat).await.is_ok() {
        match requester {
            Some(person) => format!(
                "I'm working on {}'s request at the moment",
                super::member_handover::name(tx, &person).await?
            ),
            None => "I'm in the middle of another request".to_string(),
        }
    } else {
        "I'm in the middle of another request".to_string()
    };
    let next = match others.len() {
        1 => "I'll start on this right after.".to_string(),
        n => format!("I'll get to this after {} others.", n),
    };
    post(
        tx,
        w,
        &job.chat,
        thread,
        &job.agent,
        &job.reply,
        &format!("{doing}; {next}"),
    )
    .await?;
    Ok(())
}

/// "Said: …" steps for Activity's request details, in order.
pub(crate) async fn steps(db: &impl ConnectionTrait, w: &str, reply: &str) -> Result<Vec<Value>> {
    if !ready(db).await? {
        return Ok(vec![]);
    }
    json_rows(db, "SELECT jsonb_build_object('id','said-'||m.id,'title','Said: '||left(m.content,160),'effect','update','state','completed',
        'automatic',true,'decision',NULL,'decided_by',NULL,'account',NULL,'arguments','{}'::jsonb,'at',m.at) AS data
        FROM relay_chat_messages m WHERE m.workspace=$1 AND m.update_of=$2 AND m.state<>'deleted' ORDER BY m.sequence LIMIT 20",
        vec![w.into(), reply.into()]).await
}

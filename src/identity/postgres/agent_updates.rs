//! An agent's working voice. While a request is in progress the agent can post
//! short messages about its own work — "got it, I'll…", a milestone — through
//! `send_update`; and when a request queues behind the agent's other work, the
//! front desk (`front_desk.rs`) answers in the agent's own words. Each such
//! message links to its request (`update_of`) and is never the request's
//! reply, so the reply still carries the result and the receipts. Email and
//! WhatsApp threads get neither: one complete reply (`manners`).
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

/// How an agent behaves on a channel: the built-in chat gets the human rhythm
/// (acknowledge, then updates); an email or WhatsApp thread gets one complete
/// reply when the work is done. Same agent, same work lane, different manners.
pub(crate) struct Manners {
    pub acknowledges: bool,
    pub updates: bool,
}
pub(crate) async fn manners(db: &impl ConnectionTrait, w: &str, chat: &str) -> Result<Manners> {
    let external = query(db, "SELECT to_regclass('relay_communication_conversations') IS NOT NULL
        AND EXISTS(SELECT 1 FROM relay_communication_conversations c WHERE c.workspace=$1 AND c.chat=$2) AS external", vec![w.into(), chat.into()])
        .await?
        .map(|r| r.try_get::<bool>("", "external"))
        .transpose()?
        .unwrap_or(false);
    Ok(if external {
        Manners {
            acknowledges: false,
            updates: false,
        }
    } else {
        Manners {
            acknowledges: true,
            updates: true,
        }
    })
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
pub(crate) async fn post(
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
        if !manners(&tx, &job.workspace, &job.chat).await?.updates {
            tx.commit().await?;
            return Ok(
                json!({"state":"not_here","message":"This conversation is an email or WhatsApp thread: the person gets one complete reply when the work is done. Keep working and put everything in your reply."}),
            );
        }
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

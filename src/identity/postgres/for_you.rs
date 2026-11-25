//! For you (Chat): messages addressed to the person in the channels and groups
//! they can see: @mentions, agents' replies to their requests and, since schema
//! 123, others' replies in threads they follow (`channel_threads::for_me`).
//! Direct messages are left out: each is its own row in the Chat list.
//!
//! What is visible and what is unread follow the unread counts exactly
//! (`unread.rs`), so For you, the conversation counts and the rail agree. Unread
//! items have no time limit, as the counts have none; read ones go back 90 days,
//! in pages. Each item is a bounded preview: the message is read where it is.
use super::unread::{SEEN, UNREAD_FROM, VISIBLE_WHERE};
use super::*;
use serde_json::json;

#[cfg(test)]
#[path = "for_you_tests.rs"]
mod tests;

/// Unread items returned at most, newest first.
const NEW_LIMIT: usize = 100;
/// Read items per page.
const PAGE: usize = 25;
/// How far back read items go.
const EARLIER_SECONDS: i64 = 90 * 24 * 60 * 60;
/// Characters of each message sent as its preview.
const PREVIEW: usize = 400;
/// Why an item is here, from the most specific reason.
const REASON: &str = "CASE WHEN strpos(m.content,'](mention:user:'||$2||')')>0 THEN 'mention'
 WHEN m.kind='agent' AND m.requester=$2 THEN 'agent_reply' ELSE 'thread' END";

/// Where the next page of read items starts: after this time and message.
fn cursor(before: &str) -> Result<(i64, String)> {
    let (at, id) = before.split_once(':').context("Invalid For you cursor")?;
    ensure!(!id.is_empty() && id.len() <= 100, "Invalid For you cursor");
    Ok((at.parse().context("Invalid For you cursor")?, id.to_owned()))
}

/// The fields of a hydrated message the list shows.
fn item(message: &Value, unread: bool) -> Value {
    let unavailable = message["response_unavailable"] == true;
    json!({
        "id": message["id"], "chat": message["chat"], "chat_name": message["chat_name"],
        "chat_kind": message["chat_kind"], "thread": message["thread"], "author": message["author"],
        "kind": message["kind"], "name": message["name"], "at": message["at"],
        "reason": message["reason"], "unread": unread, "unavailable": unavailable,
        "content": if unavailable { Value::String(String::new()) } else { message["content"].clone() },
    })
}

impl PgStore {
    /// The first page (`before` none) has every unread item, up to 100, and the
    /// newest read ones; later pages have only read items, older than `before`.
    pub async fn for_you(&self, user: &User, w: &str, before: Option<&str>) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        role(&tx, w, &user.id).await?;
        let for_me =
            super::channel_threads::for_me("$2", super::channel_threads::ready(&tx).await?);
        let name = "CASE WHEN m.kind='agent' THEN (SELECT a.name FROM relay_shared_agents a WHERE a.workspace=m.workspace AND a.id=m.author)
 ELSE (SELECT COALESCE(u.display_name,u.email) FROM relay_users u WHERE u.id=m.author) END";
        let select = format!(
            "SELECT jsonb_build_object('id',m.id,'chat',m.chat,'chat_name',c.name,'chat_kind',c.kind,'thread',m.thread,
               'author',m.author,'kind',m.kind,'name',{name},'state',m.state,'revision',m.revision,'at',m.at,
               'content',left(m.content,{PREVIEW}),'reason',{REASON}) AS data
             FROM {UNREAD_FROM}
             WHERE c.workspace=$1 AND c.kind<>'direct' AND {VISIBLE_WHERE} AND {for_me}"
        );
        let mut new = if before.is_none() {
            let sql =
                format!("{select} AND NOT {SEEN} ORDER BY m.at DESC, m.id DESC LIMIT {NEW_LIMIT}");
            json_rows(&tx, &sql, vec![w.into(), user.id.clone().into()]).await?
        } else {
            vec![]
        };
        let since = "extract(epoch FROM now())::bigint-";
        let mut earlier = match before.map(cursor).transpose()? {
            Some((at, id)) => {
                let sql = format!(
                    "{select} AND {SEEN} AND m.at>={since}{EARLIER_SECONDS} AND (m.at,m.id)<($3,$4)
                     ORDER BY m.at DESC, m.id DESC LIMIT {}",
                    PAGE + 1
                );
                json_rows(
                    &tx,
                    &sql,
                    vec![w.into(), user.id.clone().into(), at.into(), id.into()],
                )
                .await?
            }
            None => {
                let sql = format!(
                    "{select} AND {SEEN} AND m.at>={since}{EARLIER_SECONDS}
                     ORDER BY m.at DESC, m.id DESC LIMIT {}",
                    PAGE + 1
                );
                json_rows(&tx, &sql, vec![w.into(), user.id.clone().into()]).await?
            }
        };
        let more = earlier.len() > PAGE;
        earlier.truncate(PAGE);
        // Agent replies are checked against current source access, as in history.
        super::reply_views::hydrate(&tx, user, w, &mut new).await?;
        super::reply_views::hydrate(&tx, user, w, &mut earlier).await?;
        tx.commit().await?;
        let next_before = more
            .then(|| earlier.last())
            .flatten()
            .map(|last| format!("{}:{}", last["at"], last["id"].as_str().unwrap_or_default()));
        Ok(json!({
            "available": true,
            "new": new.iter().map(|m| item(m, true)).collect::<Vec<_>>(),
            "earlier": earlier.iter().map(|m| item(m, false)).collect::<Vec<_>>(),
            "next_before": next_before,
        }))
    }
}

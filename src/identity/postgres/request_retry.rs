//! Retry a failed request from Activity (people-and-agents-first, Phase 1b).
//!
//! A retry is a new request: the person's same message, sent again in the same
//! conversation and thread, quoting the failed reply so the agent sees what went
//! wrong and Chat and History show the link. Only the person who asked may retry,
//! so nobody starts work in someone else's name, and every ordinary intake check
//! (access, pause, membership, limits) applies again.
//!
//! A request with an action whose outcome is unknown is never retried: the action
//! may already have happened, and doing it twice could send or pay twice (D228).
//! The message id is derived from the failed request and the person, so repeated
//! clicks, lost responses and two open tabs all give the same single retry.
use super::*;
use crate::identity::chat::model::NewMessage;
use serde_json::json;

impl PgStore {
    pub(crate) async fn request_retry(&self, user: &User, w: &str, reply: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        role(&tx, w, &user.id).await?;
        let row = query(
            &tx,
            "SELECT j.state AS job_state,j.payload->>'requester' AS requester,j.payload->>'agent' AS agent,
              m.chat,m.state,r.id AS asked,r.content,r.thread,r.author,r.kind,COALESCE(r.attachments,'[]'::jsonb) AS attachments,
              EXISTS(SELECT 1 FROM relay_schedule_occurrences o WHERE o.workspace=j.workspace AND o.reply=j.reply)
               OR EXISTS(SELECT 1 FROM relay_event_deliveries d WHERE d.workspace=j.workspace AND d.reply=j.reply)
               OR EXISTS(SELECT 1 FROM relay_agent_handoffs h WHERE h.workspace=j.workspace AND h.child=j.reply) AS automatic,
              EXISTS(SELECT 1 FROM relay_control_actions x WHERE x.workspace=j.workspace AND x.job=j.reply
               AND (x.state='uncertain' OR (x.state='executing' AND x.updated<floor(extract(epoch FROM clock_timestamp()))::bigint-60))) AS unsure
             FROM relay_chat_jobs j
             JOIN relay_chat_messages m ON m.workspace=j.workspace AND m.id=j.reply
             LEFT JOIN relay_chat_messages r ON r.workspace=j.workspace AND r.id=j.request_message
             WHERE j.workspace=$1 AND j.reply=$2",
            vec![w.into(), reply.into()],
        )
        .await?
        .context("Request unavailable")?;
        let requester: String = row.try_get("", "requester")?;
        ensure!(
            requester == user.id,
            "Only the person who asked can retry this request"
        );
        let job_state: String = row.try_get("", "job_state")?;
        let state: String = row.try_get("", "state")?;
        ensure!(
            job_state == "finished" && ["failed", "interrupted"].contains(&state.as_str()),
            "Only a request that failed can be retried"
        );
        ensure!(
            !row.try_get::<bool>("", "unsure")?,
            "An action in this request may or may not have happened. Check it in the request details before asking again"
        );
        ensure!(
            !row.try_get::<bool>("", "automatic")?,
            "Schedule runs, events and hand-offs can't be retried here"
        );
        let asked: Option<String> = row.try_get("", "asked")?;
        let author: Option<String> = row.try_get("", "author")?;
        let kind: Option<String> = row.try_get("", "kind")?;
        ensure!(
            asked.is_some()
                && author.as_deref() == Some(&user.id)
                && kind.as_deref() == Some("user"),
            "Ask again in the conversation; this request has no message of yours to send again"
        );
        let attachments: Value = row.try_get("", "attachments")?;
        ensure!(
            attachments.as_array().is_none_or(Vec::is_empty),
            "This request had files. Ask again in the conversation and attach them"
        );
        let chat: String = row.try_get("", "chat")?;
        let message = NewMessage {
            id: retry_message_id(w, reply, &user.id),
            content: row.try_get("", "content")?,
            agent: Some(row.try_get("", "agent")?),
            thread: row.try_get("", "thread")?,
            reply_to: Some(reply.to_string()),
            attachments: vec![],
        };
        drop(tx);
        let retry = match self.chat_send(user, w, &chat, &message).await? {
            Some(job) => job.reply,
            // A repeat of the same retry: intake recognises the message and starts
            // nothing new, so return the request it started the first time.
            None => query(
                &self.read_tx(w).await?,
                "SELECT reply FROM relay_chat_jobs WHERE workspace=$1 AND request_message=$2",
                vec![w.into(), message.id.clone().into()],
            )
            .await?
            .context("The agent did not take the request")?
            .try_get("", "reply")?,
        };
        Ok(json!({"reply": retry, "chat": chat, "retry_of": reply}))
    }
}

/// A UUID that is the same for one person retrying one failed request, so the
/// ordinary message idempotency turns repeats into the original retry.
fn retry_message_id(w: &str, reply: &str, user: &str) -> String {
    let digest = ring::digest::digest(
        &ring::digest::SHA256,
        format!("cuckoo-retry\n{w}\n{reply}\n{user}").as_bytes(),
    );
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest.as_ref()[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    uuid::Uuid::from_bytes(bytes).to_string()
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_retry_id_is_a_stable_uuid_per_person_and_request() {
        let first = super::retry_message_id("w", "reply", "ravi");
        assert_eq!(first, super::retry_message_id("w", "reply", "ravi"));
        assert_ne!(first, super::retry_message_id("w", "reply", "meera"));
        assert_ne!(first, super::retry_message_id("w", "other", "ravi"));
        assert!(uuid::Uuid::parse_str(&first).is_ok());
    }
}

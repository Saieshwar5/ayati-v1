//! Push notifications to phones through Expo's push service (mobile plan M4).
//!
//! Every few seconds the controller asks PostgreSQL what is due
//! (`postgres/device_sessions/push_due.sql`): messages still unread ten seconds
//! after they arrived, in conversations whose level asks for them, and new
//! approvals and questions. Each item is claimed before it is sent, so it goes
//! out at most once; if Expo is down, it is not retried.
//!
//! A notification says who and what kind ("Nova replied to you") and the
//! workspace's name. It never carries message text, files or amounts. Its data
//! names only the workspace and the item, so the app opens the exact place
//! after checking access again.
use super::*;
use crate::identity::postgres::device_sessions::push::Due;
use serde_json::{Value, json};
use std::time::Duration;

const EVERY: Duration = Duration::from_secs(5);
/// Expo takes at most 100 messages per request.
const BATCH: usize = 100;

pub(super) struct Sender {
    client: reqwest::Client,
    url: String,
    token: Option<String>,
}

impl Sender {
    pub(super) fn new(settings: &crate::config::Push) -> Result<Self> {
        let token = match &settings.token_file {
            Some(path) => Some(
                std::fs::read_to_string(path)
                    .context("Cannot read AYATI_EXPO_ACCESS_TOKEN_FILE")?
                    .trim()
                    .to_owned(),
            ),
            None => None,
        };
        Ok(Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            url: settings.url.clone(),
            token,
        })
    }
}

/// The words a phone shows: who, and what kind of thing. Never the content.
pub(super) fn words(due: &Due) -> (String, String) {
    let who: String = due.author.chars().take(60).collect();
    let title = match due.reason.as_str() {
        "replied" => format!("{who} replied to you"),
        "mentioned" => format!("{who} mentioned you"),
        "direct" => format!("{who} sent you a message"),
        "approval" => format!("{who} needs your approval"),
        "question" => format!("{who} has a question for you"),
        _ => format!("{who} posted in a channel"),
    };
    (title, due.workspace_name.chars().take(80).collect())
}

/// What the app opens when the notification is tapped.
pub(super) fn target(due: &Due) -> Value {
    let mut data = json!({"workspace": due.workspace, "kind": due.kind, "chat": due.chat});
    data[due.kind.as_str()] = json!(due.reference);
    data
}

pub(super) fn messages(due: &Due, tokens: &[String]) -> Vec<Value> {
    let (title, body) = words(due);
    tokens
        .iter()
        .map(|to| {
            json!({
                "to": to, "title": title, "body": body, "data": target(due),
                "sound": "default", "priority": "high",
            })
        })
        .collect()
}

/// Send one batch. Tokens Apple or Google no longer accept are forgotten.
async fn send(app: &App, sender: &Sender, batch: &[Value]) -> Result<()> {
    let mut request = sender.client.post(&sender.url).json(batch);
    if let Some(token) = &sender.token {
        request = request.bearer_auth(token);
    }
    let response = request.send().await.context("Push service unreachable")?;
    let status = response.status();
    anyhow::ensure!(
        status.is_success(),
        "Push service answered HTTP {}",
        status.as_u16()
    );
    let answer: Value = response
        .json()
        .await
        .context("Invalid push service answer")?;
    let tickets = answer["data"].as_array().cloned().unwrap_or_default();
    for (ticket, message) in tickets.iter().zip(batch) {
        if ticket["status"] == "error" {
            let reason = ticket["details"]["error"].as_str().unwrap_or("unknown");
            if reason == "DeviceNotRegistered" {
                if let Some(token) = message["to"].as_str() {
                    app.store.forget_push_token(token).await?;
                }
            } else {
                tracing::warn!(reason, "Push notification refused");
            }
        }
    }
    Ok(())
}

/// One pass: claim what is due and send it. Returns how many were claimed.
pub(super) async fn run_once(app: &App, sender: &Sender) -> Result<usize> {
    let mut outgoing = Vec::new();
    let mut claimed = 0;
    for due in app.store.push_due().await? {
        if !app.store.claim_push(&due).await? {
            continue;
        }
        claimed += 1;
        outgoing.extend(messages(&due, &app.store.push_tokens(&due.user).await?));
    }
    for batch in outgoing.chunks(BATCH) {
        if let Err(error) = send(app, sender, batch).await {
            // Claimed items are not sent again: a missed push beats a double one.
            tracing::warn!(error = %error, count = batch.len(), "Push notifications not sent");
        }
    }
    Ok(claimed)
}

pub(super) fn start(app: Shared) -> Result<()> {
    let Some(settings) = app.config.mobile.push.clone() else {
        return Ok(());
    };
    let sender = Sender::new(&settings)?;
    tokio::spawn(async move {
        let mut passes: u64 = 0;
        loop {
            tokio::select! {
                _ = app.runtimes.stopping.cancelled() => return,
                _ = tokio::time::sleep(EVERY) => {}
            }
            if let Err(error) = run_once(&app, &sender).await {
                tracing::warn!(error = %error, "Push pass failed");
            }
            passes += 1;
            if passes.is_multiple_of(720) {
                let _ = app.store.prune_push_sent().await;
            }
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn due(reason: &str, kind: &str) -> Due {
        Due {
            workspace: "w1".into(),
            workspace_name: "Bakery".into(),
            user: "u".into(),
            kind: kind.into(),
            reference: "r1".into(),
            chat: "c1".into(),
            reason: reason.into(),
            author: "Nova".into(),
        }
    }

    #[test]
    fn notifications_say_who_and_what_kind_never_the_content() {
        assert_eq!(
            words(&due("replied", "message")),
            ("Nova replied to you".into(), "Bakery".into())
        );
        assert_eq!(
            words(&due("approval", "approval")).0,
            "Nova needs your approval"
        );
        assert_eq!(
            words(&due("channel", "message")).0,
            "Nova posted in a channel"
        );
        let sent = messages(
            &due("question", "question"),
            &["ExponentPushToken[abc]".into()],
        );
        assert_eq!(
            sent[0]["data"],
            json!({"workspace": "w1", "kind": "question", "chat": "c1", "question": "r1"})
        );
        assert_eq!(sent[0]["title"], "Nova has a question for you");
        assert_eq!(sent[0].as_object().unwrap().len(), 6);
    }
}

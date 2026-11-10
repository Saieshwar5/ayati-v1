//! The email transport (`email`), on agents' addresses at Cuckoo's mail domain.
//! Incoming mail arrives by webhook (`routes`), so `fetch` reads nothing;
//! replies go out through Postmark's Email API as plain text, threaded under
//! the message they answer. The agent never sees the provider or its token.
use super::super::adapters::{Batch, Outgoing, Route, Sent, Server, Source, TransportAdapter};
use super::settings::{self, SERVER, Settings, normal_address};
use anyhow::{Result, ensure};
use serde::Deserialize;
use serde_json::json;
use std::time::Duration;

const REASON_CHARS: usize = 300;
/// Postmark returns the Metadata it was given on delivery and bounce events.
pub(crate) const KEY: &str = "cuckoo_key";
pub(crate) const WORKSPACE: &str = "cuckoo_workspace";

pub(crate) struct EmailTransport {
    client: reqwest::Client,
    /// Tests read their own file; the controller reads the configured one.
    file: Option<std::path::PathBuf>,
}
impl EmailTransport {
    pub(crate) fn new() -> Self {
        Self {
            file: None,
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(20))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("HTTP client"),
        }
    }
    #[cfg(test)]
    pub(crate) fn with_file(file: std::path::PathBuf) -> Self {
        Self {
            file: Some(file),
            ..Self::new()
        }
    }
    fn settings(&self) -> Result<Option<Settings>> {
        match &self.file {
            Some(file) => settings::read(file).map(Some),
            None => settings::current(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Answer {
    #[serde(default)]
    error_code: i64,
    #[serde(default)]
    message: Option<String>,
    #[serde(rename = "MessageID", default)]
    message_id: Option<String>,
}

fn reason(text: String) -> String {
    text.chars().take(REASON_CHARS).collect()
}

/// `"Name" <address>`, with characters that would break the header removed.
fn mailbox(name: &str, address: &str) -> String {
    let name: String = name
        .chars()
        .filter(|c| !c.is_control() && !"\"\\<>".contains(*c))
        .take(64)
        .collect();
    match name.trim() {
        "" => address.to_owned(),
        name => format!("\"{name}\" <{address}>"),
    }
}

impl EmailTransport {
    async fn post(&self, s: &Settings, token: &str, body: serde_json::Value) -> Sent {
        let response = self
            .client
            .post(format!("{}/email", s.api()))
            .header("Accept", "application/json")
            .header("X-Postmark-Server-Token", token)
            .json(&body)
            .send()
            .await;
        let response = match response {
            Ok(r) => r,
            // A refused connection never reached Postmark; anything later might have.
            Err(e) if e.is_connect() => {
                return Sent::Unreached("Postmark could not be reached".into());
            }
            Err(_) => return Sent::Uncertain("Postmark did not answer in time".into()),
        };
        let status = response.status();
        let answer = response
            .bytes()
            .await
            .ok()
            .and_then(|b| serde_json::from_slice::<Answer>(&b).ok());
        match (status.as_u16(), answer) {
            (
                200,
                Some(Answer {
                    error_code: 0,
                    message_id,
                    ..
                }),
            ) => Sent::Accepted(message_id.map(reason)),
            (200, None) => Sent::Uncertain("Postmark's answer could not be read".into()),
            // 429 and 5xx: Postmark may or may not have queued it. Never resent.
            (429 | 408, _) | (500.., _) => {
                Sent::Uncertain(format!("Postmark answered HTTP {}", status.as_u16()))
            }
            (code, Some(a)) => Sent::Refused(reason(format!(
                "Postmark refused it ({}): {}",
                if a.error_code == 0 {
                    code as i64
                } else {
                    a.error_code
                },
                a.message.unwrap_or_default()
            ))),
            (code, None) => Sent::Refused(format!("Postmark refused it (HTTP {code})")),
        }
    }
}

#[async_trait::async_trait]
impl TransportAdapter for EmailTransport {
    fn name(&self) -> &'static str {
        "email"
    }
    fn servers(&self, _workspace: &str) -> Result<Vec<Server>> {
        Ok(match self.settings()? {
            Some(_) => vec![Server {
                id: SERVER.into(),
                name: "Email".into(),
            }],
            None => vec![],
        })
    }
    async fn check(&self, route: &Route) -> Result<()> {
        let s = self
            .settings()?
            .ok_or_else(|| anyhow::anyhow!("Email is not offered"))?;
        ensure!(
            route.server == SERVER && s.serves(&route.address),
            "This address is not on Cuckoo's mail domain"
        );
        Ok(())
    }
    /// Incoming mail arrives by webhook; there is nothing to read.
    async fn fetch(&self, _source: &Source, _cursor: Option<&str>) -> Result<Batch> {
        Ok(Batch {
            events: vec![],
            next: None,
            skipped: 0,
        })
    }
    async fn send(&self, route: &Route, outgoing: &Outgoing) -> Sent {
        let s = match self.settings() {
            Ok(Some(s)) if route.server == SERVER && s.serves(&route.address) => s,
            _ => return Sent::Unreached("This mail domain is not configured".into()),
        };
        let Ok(token) = s.token() else {
            return Sent::Unreached("The Postmark server token is unavailable".into());
        };
        let Some(to) = normal_address(&outgoing.to) else {
            return Sent::Refused("The recipient is not an email address".into());
        };
        let Some(email) = &outgoing.email else {
            return Sent::Refused("The reply has no email thread to join".into());
        };
        let name = if email.from_name.trim().is_empty() {
            &s.sender_name
        } else {
            &email.from_name
        };
        let body = json!({
            "From": mailbox(name, &route.address),
            "To": to,
            "Subject": super::inbound::reply_subject(&email.subject),
            "TextBody": outgoing.text,
            "Headers": [
                {"Name": "In-Reply-To", "Value": email.in_reply_to},
                {"Name": "References", "Value": email.references.join(" ")},
                // Tells other systems' auto-responders not to answer.
                {"Name": "Auto-Submitted", "Value": "auto-replied"},
            ],
            "MessageStream": s.message_stream,
            "TrackOpens": false,
            "TrackLinks": "None",
            "Metadata": {KEY: outgoing.key, WORKSPACE: route.workspace},
        });
        self.post(&s, &token, body).await
    }
    /// Postmark can't be asked about a send by Cuckoo's key, and an unclear
    /// send is never resent. A later delivery or bounce event still settles it.
    async fn status(&self, _route: &Route, _key: &str) -> Sent {
        Sent::Uncertain("Waiting for Postmark's delivery event".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sender_names_cannot_break_the_header() {
        assert_eq!(mailbox("Research", "r@a.test"), "\"Research\" <r@a.test>");
        assert_eq!(
            mailbox("Evil\" <x@y>\r\nBcc: z", "r@a.test"),
            "\"Evil x@yBcc: z\" <r@a.test>"
        );
        assert_eq!(mailbox("  ", "r@a.test"), "r@a.test");
    }
}

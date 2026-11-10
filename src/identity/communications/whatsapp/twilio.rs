//! The WhatsApp transport (`whatsapp`), on numbers Cuckoo registered with
//! Twilio. Incoming messages arrive by webhook (`routes`), so `fetch` reads
//! nothing; replies go out through Twilio's Messages API. The agent never sees
//! the provider, its credentials or the person's number.
use super::super::adapters::{Batch, Outgoing, Route, Sent, Server, Source, TransportAdapter};
use super::settings::{self, SERVER, STATUS_PATH, Settings};
use anyhow::{Result, ensure};
use serde::Deserialize;
use std::time::Duration;

const REASON_CHARS: usize = 300;

pub(crate) struct WhatsappTransport {
    client: reqwest::Client,
    /// Tests read their own file; the controller reads the configured one.
    file: Option<(std::path::PathBuf, String)>,
}
impl WhatsappTransport {
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
    pub(crate) fn with_file(file: std::path::PathBuf, origin: &str) -> Self {
        Self {
            file: Some((file, origin.into())),
            ..Self::new()
        }
    }
    fn settings(&self) -> Result<Option<Settings>> {
        match &self.file {
            Some((file, origin)) => settings::read(file, origin).map(Some),
            None => settings::current(),
        }
    }
}

#[derive(Deserialize)]
struct Created {
    sid: String,
}
#[derive(Deserialize)]
struct Problem {
    #[serde(default)]
    code: Option<i64>,
    #[serde(default)]
    message: Option<String>,
}

fn reason(text: String) -> String {
    text.chars().take(REASON_CHARS).collect()
}

/// The delivery-update address for one attempt. Its query is covered by
/// Twilio's signature, so the key in it can be trusted when the update arrives.
pub(crate) fn status_callback(settings: &Settings, workspace: &str, key: &str) -> String {
    let mut url = reqwest::Url::parse(&settings.url(STATUS_PATH)).expect("validated origin");
    url.query_pairs_mut()
        .append_pair("workspace", workspace)
        .append_pair("key", key);
    url.into()
}

impl WhatsappTransport {
    async fn post(&self, s: &Settings, token: &str, form: &[(&str, &str)]) -> Sent {
        let url = format!(
            "{}/2010-04-01/Accounts/{}/Messages.json",
            s.api(),
            s.account
        );
        let response = self
            .client
            .post(url)
            .basic_auth(&s.account, Some(token))
            .form(form)
            .send()
            .await;
        let response = match response {
            Ok(r) => r,
            // A refused connection never reached Twilio; anything later might have.
            Err(e) if e.is_connect() => {
                return Sent::Unreached("Twilio could not be reached".into());
            }
            Err(_) => return Sent::Uncertain("Twilio did not answer in time".into()),
        };
        let status = response.status();
        let body = response.bytes().await.unwrap_or_default();
        if status.is_success() {
            return match serde_json::from_slice::<Created>(&body) {
                Ok(created) => Sent::Accepted(Some(reason(created.sid))),
                Err(_) => Sent::Uncertain("Twilio's answer could not be read".into()),
            };
        }
        if status.is_client_error() && status.as_u16() != 429 && status.as_u16() != 408 {
            let problem = serde_json::from_slice::<Problem>(&body).ok();
            let text = match problem {
                Some(Problem {
                    code: Some(code),
                    message,
                }) => format!(
                    "Twilio refused it ({code}): {}",
                    message.unwrap_or_default()
                ),
                _ => format!("Twilio refused it (HTTP {})", status.as_u16()),
            };
            return Sent::Refused(reason(text));
        }
        // 429 and 5xx: Twilio may or may not have queued it. Never resent.
        Sent::Uncertain(format!("Twilio answered HTTP {}", status.as_u16()))
    }
}

#[async_trait::async_trait]
impl TransportAdapter for WhatsappTransport {
    fn name(&self) -> &'static str {
        "whatsapp"
    }
    fn servers(&self, _workspace: &str) -> Result<Vec<Server>> {
        Ok(match self.settings()? {
            Some(_) => vec![Server {
                id: SERVER.into(),
                name: "WhatsApp".into(),
            }],
            None => vec![],
        })
    }
    async fn check(&self, route: &Route) -> Result<()> {
        let s = self
            .settings()?
            .ok_or_else(|| anyhow::anyhow!("WhatsApp is not offered"))?;
        ensure!(
            route.server == SERVER && s.offers(&route.address),
            "This WhatsApp number is not one of Cuckoo's"
        );
        Ok(())
    }
    /// Incoming messages arrive by webhook; there is nothing to read.
    async fn fetch(&self, _source: &Source, _cursor: Option<&str>) -> Result<Batch> {
        Ok(Batch {
            events: vec![],
            next: None,
            skipped: 0,
        })
    }
    async fn send(&self, route: &Route, outgoing: &Outgoing) -> Sent {
        let s = match self.settings() {
            Ok(Some(s)) if route.server == SERVER && s.offers(&route.address) => s,
            _ => return Sent::Unreached("This WhatsApp number is not configured".into()),
        };
        let Ok(token) = s.token() else {
            return Sent::Unreached("The Twilio auth token is unavailable".into());
        };
        if !settings::valid_number(&outgoing.to) {
            return Sent::Refused("The recipient is not a WhatsApp number".into());
        }
        let parts = match super::text::parts(&super::text::whatsapp(&outgoing.text)) {
            Ok(parts) => parts,
            Err(e) => return Sent::Refused(e.to_string()),
        };
        let from = format!("whatsapp:{}", route.address);
        let to = format!("whatsapp:{}", outgoing.to);
        let callback = status_callback(&s, &route.workspace, &outgoing.key);
        let mut first = None;
        for (i, part) in parts.iter().enumerate() {
            let sent = self
                .post(
                    &s,
                    &token,
                    &[
                        ("From", &from),
                        ("To", &to),
                        ("Body", part),
                        ("StatusCallback", &callback),
                    ],
                )
                .await;
            match sent {
                Sent::Accepted(sid) if i == 0 => first = sid,
                Sent::Accepted(_) => {}
                // The first part decides; after it, the person already has part
                // of the reply, so a failure is reported but never resent.
                other if i == 0 => return other,
                _ => {
                    return Sent::Uncertain(format!("Only {i} of {} parts were sent", parts.len()));
                }
            }
        }
        Sent::Accepted(first)
    }
    /// Twilio can't be asked about a send by Cuckoo's key, and an unclear send
    /// is never resent. A later delivery update still settles it.
    async fn status(&self, _route: &Route, _key: &str) -> Sent {
        Sent::Uncertain("Waiting for Twilio's delivery update".into())
    }
}

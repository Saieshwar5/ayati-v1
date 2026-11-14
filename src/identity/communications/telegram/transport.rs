//! The Telegram transport (`telegram`): agent replies go out through each
//! agent's own bot. Incoming updates are read by `poller`, so `fetch` reads
//! nothing here. The endpoint's address is the bot's numeric ID; its token is
//! asked from Telegram by Cuckoo's manager bot and never stored.
use super::super::adapters::{Batch, Outgoing, Route, Sent, Server, Source, TransportAdapter};
use super::api::{self, Answer, Api};
use super::settings::{self, SERVER, Settings};
use anyhow::Result;

pub(crate) struct TelegramTransport {
    /// Tests read their own file; the controller reads the configured one.
    file: Option<std::path::PathBuf>,
}
impl TelegramTransport {
    pub(crate) fn new() -> Self {
        Self { file: None }
    }
    #[cfg(test)]
    pub(crate) fn with_file(file: std::path::PathBuf) -> Self {
        Self { file: Some(file) }
    }
    fn settings(&self) -> Result<Option<Settings>> {
        match &self.file {
            Some(file) => settings::read(file).map(Some),
            None => settings::current(),
        }
    }
}

/// What Cuckoo records for one part Telegram answered.
fn sent(answer: Answer<api::Sent>, bot: i64) -> Sent {
    match answer {
        Answer::Done(m) => Sent::Accepted(Some(m.message_id.to_string())),
        Answer::Refused {
            code: 429,
            retry_after,
            ..
        } => Sent::Unreached(format!(
            "Telegram asked Cuckoo to wait {} seconds",
            retry_after.unwrap_or(30)
        )),
        Answer::Refused {
            code, description, ..
        } => {
            if code == 401 {
                api::forget(bot);
            }
            Sent::Refused(format!("Telegram refused it ({code}): {description}"))
        }
        Answer::Unreached => Sent::Unreached("Telegram could not be reached".into()),
        Answer::Unclear(reason) => Sent::Uncertain(reason),
    }
}

#[async_trait::async_trait]
impl TransportAdapter for TelegramTransport {
    fn name(&self) -> &'static str {
        "telegram"
    }
    fn servers(&self, _workspace: &str) -> Result<Vec<Server>> {
        Ok(match self.settings()? {
            Some(_) => vec![Server {
                id: SERVER.into(),
                name: "Telegram".into(),
            }],
            None => vec![],
        })
    }
    /// Telegram endpoints are made only by connecting an agent's bot.
    async fn check(&self, _route: &Route) -> Result<()> {
        anyhow::bail!("Switch on Telegram from the agent's page")
    }
    async fn fetch(&self, _source: &Source, _cursor: Option<&str>) -> Result<Batch> {
        Ok(Batch {
            events: vec![],
            next: None,
            skipped: 0,
        })
    }
    async fn send(&self, route: &Route, outgoing: &Outgoing) -> Sent {
        let (Ok(Some(s)), Ok(bot)) = (self.settings(), route.address.parse::<i64>()) else {
            return Sent::Unreached("Telegram is not configured".into());
        };
        if route.server != SERVER {
            return Sent::Unreached("Telegram is not configured".into());
        }
        let Ok(chat) = outgoing.to.parse::<i64>() else {
            return Sent::Refused("The recipient is not a Telegram chat".into());
        };
        let Ok(manager) = s.token() else {
            return Sent::Unreached("The Telegram manager token is unavailable".into());
        };
        let api = Api::new(s.api());
        let token = match api::bot_token(&api, &manager, bot).await {
            Answer::Done(token) => token,
            Answer::Refused { description, .. } => {
                return Sent::Refused(format!("Telegram refused the agent's bot: {description}"));
            }
            Answer::Unreached => return Sent::Unreached("Telegram could not be reached".into()),
            // Nothing was sent yet, so asking again later is safe.
            Answer::Unclear(_) => {
                return Sent::Unreached("Telegram did not give the bot's token".into());
            }
        };
        let parts = match super::text::parts(&outgoing.text) {
            Ok(parts) => parts,
            Err(e) => return Sent::Refused(e.to_string()),
        };
        let mut first = None;
        for (i, part) in parts.iter().enumerate() {
            let mut answer = api.send(&token, chat, part, true, None).await;
            // Refused formatting means nothing was sent: send it once as plain text.
            if let Answer::Refused {
                code: 400,
                description,
                ..
            } = &answer
                && description.contains("parse entities")
            {
                answer = api
                    .send(&token, chat, &super::text::plain(part), false, None)
                    .await;
            }
            match sent(answer, bot) {
                Sent::Accepted(id) if i == 0 => first = id,
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
    /// Telegram can't be asked about a send, and an unclear send is never resent.
    async fn status(&self, _route: &Route, _key: &str) -> Sent {
        Sent::Uncertain("Telegram cannot be asked about an unclear send".into())
    }
}

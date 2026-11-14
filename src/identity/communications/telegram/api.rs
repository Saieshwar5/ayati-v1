//! A small Telegram Bot API client: the few methods Cuckoo uses, the update
//! shapes it reads, and the managed bots' tokens. A token is part of every
//! request URL, so no request error, URL or token is ever logged or saved.
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
    time::Duration,
};

const REASON_CHARS: usize = 300;

/// What Telegram said.
#[derive(Debug)]
pub(crate) enum Answer<T> {
    Done(T),
    /// Telegram refused the request; nothing happened.
    Refused {
        code: u16,
        description: String,
        retry_after: Option<i64>,
    },
    /// The request never reached Telegram.
    Unreached,
    /// It may or may not have reached Telegram.
    Unclear(String),
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct Update {
    pub update_id: i64,
    #[serde(default)]
    pub message: Option<Message>,
    #[serde(default)]
    pub managed_bot: Option<ManagedBot>,
}
#[derive(Clone, Debug, Deserialize)]
pub(crate) struct Message {
    #[serde(default)]
    pub from: Option<User>,
    pub chat: Chat,
    #[serde(default)]
    pub text: Option<String>,
}
#[derive(Clone, Debug, Deserialize)]
pub(crate) struct Chat {
    pub id: i64,
    #[serde(rename = "type")]
    pub kind: String,
}
#[derive(Clone, Debug, Deserialize)]
pub(crate) struct User {
    pub id: i64,
    #[serde(default)]
    pub is_bot: bool,
    #[serde(default)]
    pub username: Option<String>,
}
/// A bot created to be managed by Cuckoo's manager bot, or one whose token or
/// owner changed.
#[derive(Clone, Debug, Deserialize)]
pub(crate) struct ManagedBot {
    pub user: User,
    pub bot: User,
}
#[derive(Debug, Deserialize)]
pub(crate) struct Sent {
    pub message_id: i64,
}

#[derive(Deserialize)]
struct Envelope {
    ok: bool,
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error_code: Option<u16>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    parameters: Option<Parameters>,
}
#[derive(Deserialize)]
struct Parameters {
    #[serde(default)]
    retry_after: Option<i64>,
}

#[derive(Clone)]
pub(crate) struct Api {
    client: reqwest::Client,
    base: String,
}

impl Api {
    pub(crate) fn new(base: &str) -> Self {
        Self {
            base: base.trim_end_matches('/').to_owned(),
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(20))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("HTTP client"),
        }
    }

    async fn call<T: DeserializeOwned>(&self, token: &str, method: &str, body: Value) -> Answer<T> {
        let url = format!("{}/bot{token}/{method}", self.base);
        let response = match self.client.post(url).json(&body).send().await {
            Ok(r) => r,
            // A refused connection never reached Telegram; anything later might have.
            Err(e) if e.is_connect() => return Answer::Unreached,
            Err(_) => return Answer::Unclear("Telegram did not answer in time".into()),
        };
        let status = response.status().as_u16();
        let Ok(envelope) = response.json::<Envelope>().await else {
            return Answer::Unclear(format!(
                "Telegram's answer (HTTP {status}) could not be read"
            ));
        };
        if envelope.ok {
            return match envelope.result.map(serde_json::from_value) {
                Some(Ok(result)) => Answer::Done(result),
                _ => Answer::Unclear("Telegram's answer could not be read".into()),
            };
        }
        let code = envelope.error_code.unwrap_or(status);
        let description: String = envelope
            .description
            .unwrap_or_default()
            .chars()
            .take(REASON_CHARS)
            .collect();
        if code >= 500 {
            return Answer::Unclear(format!("Telegram answered {code}"));
        }
        Answer::Refused {
            code,
            description,
            retry_after: envelope.parameters.and_then(|p| p.retry_after),
        }
    }

    /// Updates after `next`. Short polling: several controllers take turns
    /// with a lease, never two at once.
    pub(crate) async fn updates(
        &self,
        token: &str,
        next: i64,
        kinds: &[&str],
    ) -> Answer<Vec<Update>> {
        self.call(
            token,
            "getUpdates",
            json!({"offset": next, "limit": 50, "timeout": 0, "allowed_updates": kinds}),
        )
        .await
    }

    pub(crate) async fn send(
        &self,
        token: &str,
        chat: i64,
        text: &str,
        html: bool,
        markup: Option<Value>,
    ) -> Answer<Sent> {
        let mut body =
            json!({"chat_id": chat, "text": text, "link_preview_options": {"is_disabled": true}});
        if html {
            body["parse_mode"] = json!("HTML");
        }
        if let Some(markup) = markup {
            body["reply_markup"] = markup;
        }
        self.call(token, "sendMessage", body).await
    }

    pub(crate) async fn managed_token(&self, manager: &str, bot: i64) -> Answer<String> {
        self.call(manager, "getManagedBotToken", json!({"user_id": bot}))
            .await
    }

    pub(crate) async fn leave(&self, token: &str, chat: i64) -> Answer<bool> {
        self.call(token, "leaveChat", json!({"chat_id": chat}))
            .await
    }

    /// The bot's own profile: what people see before they press Start.
    pub(crate) async fn describe(&self, token: &str, description: &str, short: &str) {
        let _: Answer<bool> = self
            .call(
                token,
                "setMyDescription",
                json!({"description": description}),
            )
            .await;
        let _: Answer<bool> = self
            .call(
                token,
                "setMyShortDescription",
                json!({"short_description": short}),
            )
            .await;
        let _: Answer<bool> = self
            .call(
                token,
                "setMyCommands",
                json!({"commands": [{"command": "start", "description": "Start talking to this agent"}]}),
            )
            .await;
    }
}

/// Managed bots' tokens, asked from Telegram by the manager bot and kept only
/// in this process's memory.
static TOKENS: LazyLock<Mutex<HashMap<i64, String>>> = LazyLock::new(Default::default);

pub(crate) async fn bot_token(api: &Api, manager: &str, bot: i64) -> Answer<String> {
    if let Some(token) = TOKENS.lock().unwrap().get(&bot).cloned() {
        return Answer::Done(token);
    }
    let answer = api.managed_token(manager, bot).await;
    if let Answer::Done(token) = &answer {
        if !super::settings::bot_token(token) {
            return Answer::Unclear("Telegram returned an unreadable bot token".into());
        }
        TOKENS.lock().unwrap().insert(bot, token.clone());
    }
    answer
}

/// A bot's token changed or was refused: ask Telegram again next time.
pub(crate) fn forget(bot: i64) {
    TOKENS.lock().unwrap().remove(&bot);
}

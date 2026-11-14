//! Reads Telegram every few seconds on each controller: Cuckoo's manager bot
//! (Start codes and newly created agent bots) and every active agent bot
//! (people's messages). Each bot is read by one controller at a time under a
//! lease, and its position moves only after its updates are saved, so a crash
//! rereads them and duplicates are dropped. Fixed replies are never agent work.
use super::api::{self, Answer, Api, Message, Update};
use super::settings::{self, Settings};
use crate::identity::postgres::{
    PgStore, TelegramCreated, TelegramFrom, TelegramIncoming, TelegramLinked, TelegramOutcome,
    telegram_start_code,
};
use serde_json::json;
use std::time::Duration;

const EVERY: Duration = Duration::from_secs(3);

pub(crate) fn start(app: super::super::super::Shared) {
    tokio::spawn(async move {
        let stopping = app.runtimes.stopping.clone();
        let mut tick = tokio::time::interval(EVERY);
        loop {
            tokio::select! {
                _ = stopping.cancelled() => break,
                _ = tick.tick() => {}
            }
            let Ok(Some(s)) = settings::current() else {
                continue;
            };
            let round = run_once(&app.store, &s).await;
            if round.admitted > 0 {
                app.runtimes.chat_wake.notify_one();
            }
            if round.admitted + round.refused + round.linked + round.created > 0 {
                tracing::info!(
                    round.admitted,
                    round.refused,
                    round.linked,
                    round.created,
                    "Telegram"
                );
            }
        }
    });
}

/// What one round did, for logs and tests. Counts only, never content.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Round {
    pub admitted: usize,
    pub refused: usize,
    pub linked: usize,
    pub created: usize,
}

pub(crate) async fn run_once(store: &PgStore, s: &Settings) -> Round {
    let mut round = Round::default();
    let Ok(manager) = s.token() else {
        tracing::warn!("The Telegram manager token is unavailable");
        return round;
    };
    let api = Api::new(s.api());
    manager_round(store, &api, &manager, &mut round).await;
    bots_round(store, &api, &manager, &mut round).await;
    round
}

fn transient(error: &anyhow::Error) -> bool {
    error.chain().any(|e| e.is::<sea_orm::DbErr>())
}

async fn reply(api: &Api, token: &str, chat: i64, text: &str) {
    let _ = api.send(token, chat, text, false, None).await;
}

/// The text of a private message from a person, or why it is skipped.
fn private(message: &Message) -> Option<(i64, &api::User)> {
    let from = message.from.as_ref().filter(|f| !f.is_bot)?;
    (message.chat.kind == "private").then_some((message.chat.id, from))
}

async fn manager_round(store: &PgStore, api: &Api, manager: &str, round: &mut Round) {
    let Ok(Some(next)) = store.telegram_manager_claim().await else {
        return;
    };
    let mut last = next;
    if let Answer::Done(updates) = api
        .updates(manager, next, &["message", "managed_bot"])
        .await
    {
        for update in updates {
            if manager_update(store, api, manager, &update, round)
                .await
                .is_err()
            {
                break;
            }
            last = update.update_id + 1;
        }
    }
    if store.telegram_manager_release(last).await.is_err() {
        tracing::warn!(
            "The Telegram manager position could not be saved; its updates will be read again"
        );
    }
}

async fn manager_update(
    store: &PgStore,
    api: &Api,
    manager: &str,
    update: &Update,
    round: &mut Round,
) -> anyhow::Result<()> {
    if let Some(managed) = &update.managed_bot {
        // A new bot, or a managed bot whose token or owner changed.
        api::forget(managed.bot.id);
        let created = match store
            .telegram_bot_created(
                managed.user.id,
                managed.bot.id,
                managed.bot.username.as_deref(),
            )
            .await
        {
            Err(e) if transient(&e) => return Err(e),
            Err(e) => {
                tracing::info!(error = %e, "A new Telegram bot could not be attached");
                TelegramCreated::Unmatched
            }
            Ok(created) => created,
        };
        match created {
            TelegramCreated::Attached {
                bot,
                agent,
                workspace,
            } => {
                round.created += 1;
                if let (Answer::Done(token), Ok((_, description))) = (
                    api::bot_token(api, manager, bot).await,
                    store.telegram_bot_profile(&workspace, &agent).await,
                ) {
                    let short: String = description.chars().take(120).collect();
                    api.describe(&token, &description, &short).await;
                }
                let username = managed.bot.username.as_deref().unwrap_or("the new bot");
                let _ = api
                    .send(
                        manager,
                        managed.user.id,
                        &format!("Done. The agent is ready on Telegram: @{username}"),
                        false,
                        Some(json!({"remove_keyboard": true})),
                    )
                    .await;
            }
            TelegramCreated::Known => {}
            TelegramCreated::Unmatched => {
                reply(api, manager, managed.user.id, "I couldn't match this bot to an agent. Switch on Telegram for the agent in Cuckoo first, then create its bot from the button I send you.").await;
            }
        }
        return Ok(());
    }
    let Some(message) = &update.message else {
        return Ok(());
    };
    if message.chat.kind != "private" {
        let _ = api.leave(manager, message.chat.id).await;
        return Ok(());
    }
    let Some((chat, from)) = private(message) else {
        return Ok(());
    };
    let text = message.text.as_deref().unwrap_or_default();
    let code = text.strip_prefix("/start").and_then(telegram_start_code);
    let Some(code) = code else {
        reply(api, manager, chat, "I'm Cuckoo's Telegram helper. Connect Telegram from your Cuckoo profile, then open an agent in Cuckoo and choose Open in Telegram.").await;
        return Ok(());
    };
    let from = TelegramFrom {
        id: from.id,
        username: from.username.clone(),
    };
    let linked = match store.telegram_start(code, &from).await {
        Err(e) if transient(&e) => return Err(e),
        Err(e) => {
            tracing::info!(error = %e, "A Telegram link could not be saved");
            TelegramLinked::Invalid
        }
        Ok(linked) => linked,
    };
    let text = match linked {
        TelegramLinked::Person(person) => {
            round.linked += 1;
            format!(
                "You're connected, {person}. Open an agent in Cuckoo and choose Open in Telegram to talk to it here."
            )
        }
        TelegramLinked::Owner { person, workspace } => {
            round.linked += 1;
            format!(
                "You're connected, {person}. You now create the Telegram bots for {workspace}'s agents: when someone switches on Telegram for an agent, I'll send you a button here."
            )
        }
        TelegramLinked::Invalid => {
            "This link has expired or was already used. Open Cuckoo and try again.".into()
        }
        TelegramLinked::Taken => {
            "This Telegram account is already connected to another Cuckoo account.".into()
        }
    };
    reply(api, manager, chat, &text).await;
    Ok(())
}

fn refusal(workspace: &str) -> String {
    format!(
        "This assistant is only for members of {workspace} who may use it. Connect Telegram in your Cuckoo profile, or ask your workspace admin for access."
    )
}

async fn bots_round(store: &PgStore, api: &Api, manager: &str, round: &mut Round) {
    let Ok(claims) = store.telegram_bot_claims().await else {
        return;
    };
    for polled in claims {
        let mut last = polled.next;
        if let Answer::Done(token) = api::bot_token(api, manager, polled.bot).await {
            match api.updates(&token, polled.next, &["message"]).await {
                Answer::Done(updates) => {
                    for update in updates {
                        if bot_update(store, api, &token, polled.bot, &update, round)
                            .await
                            .is_err()
                        {
                            break;
                        }
                        last = update.update_id + 1;
                    }
                }
                // A token Telegram no longer accepts is asked for again next time.
                Answer::Refused { code: 401, .. } => api::forget(polled.bot),
                _ => {}
            }
        }
        if store.telegram_bot_release(polled.bot, last).await.is_err() {
            tracing::warn!(workspace = %polled.workspace, "A Telegram bot's position could not be saved; its updates will be read again");
        }
    }
}

async fn bot_update(
    store: &PgStore,
    api: &Api,
    token: &str,
    bot: i64,
    update: &Update,
    round: &mut Round,
) -> anyhow::Result<()> {
    let Some(message) = &update.message else {
        return Ok(());
    };
    // An agent's bot works only in private chats; in a group, being there is no permission.
    if message.chat.kind != "private" {
        let _ = api.leave(token, message.chat.id).await;
        return Ok(());
    }
    let Some((chat, from)) = private(message) else {
        return Ok(());
    };
    let Some(text) = message.text.as_deref() else {
        reply(api, token, chat, "I can only read text messages for now.").await;
        return Ok(());
    };
    if text == "/start" || text.starts_with("/start ") {
        let (welcome, workspace) = store.telegram_greeting(bot, from.id).await?;
        match (welcome, workspace) {
            (Some((person, agent)), _) => {
                reply(
                    api,
                    token,
                    chat,
                    &format!("Hi {person}, I'm {agent}. Ask me anything."),
                )
                .await
            }
            (None, Some(workspace)) => {
                round.refused += 1;
                reply(api, token, chat, &refusal(&workspace)).await;
            }
            (None, None) => {}
        }
        return Ok(());
    }
    let input = TelegramIncoming {
        bot,
        update: update.update_id,
        from: from.id,
        chat,
        text: text.to_owned(),
    };
    match store.telegram_incoming(&input).await {
        Err(e) if transient(&e) => return Err(e),
        // Cuckoo refuses it (for example too long); it is not read again.
        Err(_) => {
            reply(
                api,
                token,
                chat,
                "That message is too long for me. Please send a shorter one.",
            )
            .await
        }
        Ok(TelegramOutcome::Admitted) => round.admitted += 1,
        Ok(TelegramOutcome::Held { workspace }) => {
            round.refused += 1;
            reply(api, token, chat, &refusal(&workspace)).await;
        }
        Ok(TelegramOutcome::Duplicate | TelegramOutcome::Ignored) => {}
    }
    Ok(())
}

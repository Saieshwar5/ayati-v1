//! Telegram for people inside the organisation: one bot per agent, created by
//! the workspace's Telegram owner with one tap and managed by Cuckoo's own
//! manager bot. The operator's settings, a small Bot API client, Telegram
//! formatting, the transport that sends replies, the poller that reads every
//! bot, and the routes. See `runtime/telegram-channel.md`.
pub(crate) mod api;
pub(crate) mod poller;
pub(crate) mod routes;
pub(crate) mod settings;
mod text;
pub(crate) mod transport;

/// Whether this server offers Telegram.
pub(crate) fn offered() -> bool {
    matches!(settings::current(), Ok(Some(_)))
}

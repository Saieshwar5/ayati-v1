//! WhatsApp on numbers Cuckoo provides: the operator's settings, the Twilio
//! transport, WhatsApp text formatting and the routes (switching an agent's
//! WhatsApp on and off, linking a person's number, Twilio's webhooks).
//! See `runtime/whatsapp-channel.md`.
pub(crate) mod routes;
pub(crate) mod settings;
mod text;
pub(crate) mod twilio;

/// Twilio's signed webhooks; exempt from the browser Origin checks like the
/// other provider intakes.
pub(crate) fn webhook_path(path: &str) -> bool {
    path == settings::INCOMING_PATH || path == settings::STATUS_PATH
}

/// Whether this server can give agents WhatsApp numbers.
pub(crate) fn offered() -> bool {
    matches!(settings::current(), Ok(Some(s)) if !s.numbers.is_empty())
}

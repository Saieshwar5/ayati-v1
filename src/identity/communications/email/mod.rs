//! Email on addresses at Cuckoo's mail domain: the operator's settings, reading
//! Postmark's inbound mail, the Postmark transport and the routes (switching an
//! agent's email on and off, Postmark's webhooks). See `runtime/email-channel.md`.
pub(crate) mod inbound;
pub(crate) mod postmark;
pub(crate) mod routes;
pub(crate) mod settings;

/// Postmark's webhooks; exempt from the browser Origin checks like the other
/// provider intakes. They are authorized by their Basic credentials.
pub(crate) fn webhook_path(path: &str) -> bool {
    path == settings::INCOMING_PATH || path == settings::EVENTS_PATH
}

/// Whether this server can give agents email addresses.
pub(crate) fn offered() -> bool {
    matches!(settings::current(), Ok(Some(_)))
}

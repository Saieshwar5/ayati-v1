//! Agent email through Amazon SES: the operator's settings, reading SES's
//! queue, parsing incoming mail, composing replies and the SES transport.
//! See `runtime/email-channel.md`.
pub(crate) mod compose;
pub(crate) mod inbound;
pub(crate) mod notification;
pub(crate) mod queue;
pub(crate) mod routes;
pub(crate) mod ses;
pub(crate) mod settings;

/// Whether this server can give agents email addresses.
pub(crate) fn offered() -> bool {
    matches!(settings::current(), Ok(Some(_)))
}

//! Communication records: endpoints and who may write to them, the inbox and
//! outbox, and how far the gateway has read each transport. A transport's
//! message is evidence of a sender; the endpoint decides workspace and agent.
use super::*;
pub(super) mod access;
mod delivery;
mod email_addresses;
mod email_people;
#[cfg(test)]
mod email_tests;
mod endpoints;
#[cfg(test)]
mod gateway_tests;
pub(super) mod inbox;
#[cfg(test)]
mod intake_tests;
pub(super) mod migration;
pub(super) mod outbox;
#[cfg(test)]
mod recovery_tests;
mod whatsapp_numbers;
mod whatsapp_people;
#[cfg(test)]
mod whatsapp_tests;
pub(crate) use email_people::Outcome as EmailOutcome;
pub(crate) use whatsapp_numbers::{NoFreeNumber, Offer};
pub(crate) use whatsapp_people::{Incoming as WhatsappIncoming, Outcome as WhatsappOutcome};

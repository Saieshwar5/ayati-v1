//! Communication records: endpoints and who may write to them, the inbox and
//! outbox, and how far the gateway has read each transport. A transport's
//! message is evidence of a sender; the endpoint decides workspace and agent.
use super::*;
pub(super) mod access;
mod delivery;
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
mod telegram_bots;
#[cfg(test)]
mod telegram_fake;
mod telegram_people;
#[cfg(test)]
mod telegram_people_tests;
#[cfg(test)]
mod telegram_tests;
mod whatsapp_numbers;
mod whatsapp_people;
#[cfg(test)]
mod whatsapp_tests;
pub(crate) use telegram_bots::{
    Created as TelegramCreated, NotConnected as TelegramNotConnected, OwnerAsk as TelegramOwnerAsk,
};
pub(crate) use telegram_people::{
    From as TelegramFrom, Incoming as TelegramIncoming, Linked as TelegramLinked,
    Outcome as TelegramOutcome, start_code as telegram_start_code,
};
pub(crate) use whatsapp_numbers::{NoFreeNumber, Offer};
pub(crate) use whatsapp_people::{Incoming as WhatsappIncoming, Outcome as WhatsappOutcome};

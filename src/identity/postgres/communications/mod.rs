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

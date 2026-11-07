//! Wake-ups for the controller's background loops (schema 97).
//!
//! Until schema 97 the loops polled: the executor dispatcher every 200 ms, the
//! scheduler every second and the transcription worker every 5 seconds, which
//! kept an idle database busy (D226). Now database triggers announce work on one
//! channel in the transaction that changes it: a job or executor task added, put
//! back or finished, an executor slot ready, a voice transcript queued or done,
//! and anything the scheduler admits (schedules, events, Library arrivals, app
//! records, follow-ups, panel updates, helper waits). PostgreSQL sends a
//! notification at commit, at most once per transaction for each loop.
//!
//! Each controller listens once (`messaging.rs`) and wakes the loop the
//! notification names. Notifications are hints, never authority: the loops still
//! read everything from the database, and each keeps a slow fallback timer in
//! case one is lost. A notification names its schema, so controllers sharing a
//! database through separate schemas (tests, simulations) wake only for theirs.
use sea_orm::ConnectionTrait;
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};
use tokio::sync::Notify;

#[cfg(test)]
#[path = "work_signals_tests.rs"]
mod tests;

/// The PostgreSQL channel; each payload is `<schema>/<loop>`.
pub(crate) const CHANNEL: &str = "relay_work";

/// One wake-up per background loop. `Notify` keeps a single permit, so a burst
/// of notifications while a loop is busy becomes one more look, not many.
#[derive(Default)]
pub(crate) struct WorkSignals {
    /// The executor dispatcher (`worker_control::pool::dispatch`).
    pub dispatch: Notify,
    /// The scheduler (`automations::scheduler`).
    pub schedule: Notify,
    /// The voice transcription worker.
    pub transcribe: Notify,
    /// The chat file reader (schema 100).
    pub read: Notify,
    /// The presence sweep: someone came online and can now go offline (schema 98).
    pub presence: Notify,
}
impl WorkSignals {
    /// Wakes the loop a notification for `schema` names; one for another schema
    /// is ignored, and an unknown loop wakes them all.
    pub(crate) fn notified(&self, schema: &str, payload: &str) {
        let Some(kind) = payload
            .strip_prefix(schema)
            .and_then(|rest| rest.strip_prefix('/'))
        else {
            return;
        };
        match kind {
            "dispatch" => self.dispatch.notify_one(),
            "schedule" => self.schedule.notify_one(),
            "transcribe" => self.transcribe.notify_one(),
            "read" => self.read.notify_one(),
            "presence" => self.presence.notify_one(),
            _ => self.wake_all(),
        }
    }
    /// When the listener (re)connects: anything announced while it was away.
    pub(crate) fn wake_all(&self) {
        self.dispatch.notify_one();
        self.schedule.notify_one();
        self.transcribe.notify_one();
        self.read.notify_one();
        self.presence.notify_one();
    }
}

pub(crate) struct WorkNotifications;
impl MigrationName for WorkNotifications {
    fn name(&self) -> &str {
        "m20261107_000097_work_notifications"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for WorkNotifications {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("work_signals.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Work notifications require a forward migration".into(),
        ))
    }
}

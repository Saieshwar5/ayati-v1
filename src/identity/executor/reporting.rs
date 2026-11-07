//! When the executor reports its capacity. A report is also its heartbeat
//! (`relay_executors.heartbeat`), so it goes at once when a slot changes or
//! draining begins, and otherwise every `HEARTBEAT`. Until November 2026 every
//! report went every second, about five transactions a second per executor.
use super::model::Capacity;
use std::time::Duration;
use tokio::time::Instant;

/// Unchanged capacity is reported this often. The controller withdraws an
/// executor after `AYATI_EXECUTOR_EXPIRY_SECONDS` without a report (45 by
/// default, at least 40), so only three or more missed heartbeats expire it.
/// The health probe allows the same (`health::STALE_SECONDS`).
pub(crate) const HEARTBEAT: Duration = Duration::from_secs(10);

/// The last report the controller accepted, and when it was sent.
#[derive(Default)]
pub(super) struct Reports {
    accepted: Option<(Capacity, bool, Instant)>,
}
impl Reports {
    /// Due when the report differs from the accepted one or the heartbeat is
    /// due. A refused or failed report is never accepted, so it is retried at
    /// the next local check (every second).
    pub fn due(&self, capacity: &Capacity, draining: bool, now: Instant) -> bool {
        match &self.accepted {
            Some((last, was_draining, at)) => {
                last != capacity || *was_draining != draining || now - *at >= HEARTBEAT
            }
            None => true,
        }
    }
    pub fn accepted(&mut self, capacity: Capacity, draining: bool, sent: Instant) {
        self.accepted = Some((capacity, draining, sent));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::executor::model::{Profile, SlotReport};

    fn capacity(state: &str) -> Capacity {
        Capacity {
            slots: vec![SlotReport {
                slot: "slot".into(),
                generation: 1,
                profile: Profile {
                    provider: "openai".into(),
                    model: "fixture".into(),
                },
                state: state.into(),
                token: "token".into(),
                pid: Some(7),
                preparation_version: 1,
            }],
        }
    }

    #[test]
    fn a_capacity_change_is_reported_immediately_and_unchanged_capacity_every_heartbeat() {
        let start = Instant::now();
        let mut reports = Reports::default();
        assert!(reports.due(&capacity("preparing"), false, start), "first");
        reports.accepted(capacity("preparing"), false, start);
        assert!(!reports.due(&capacity("preparing"), false, start + HEARTBEAT / 2));
        let later = start + Duration::from_secs(1);
        assert!(
            reports.due(&capacity("ready"), false, later),
            "a slot became ready"
        );
        assert!(
            reports.due(&capacity("preparing"), true, later),
            "draining began"
        );
        assert!(reports.due(&capacity("preparing"), false, start + HEARTBEAT));
        // A report the controller refused is not accepted: the change stays due.
        assert!(reports.due(&capacity("running"), false, later));
        reports.accepted(capacity("running"), false, later);
        assert!(!reports.due(&capacity("running"), false, later + HEARTBEAT / 2));
    }
}

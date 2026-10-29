//! Local probe for `ayati executor-health`. The executor has no listening port,
//! so the supervisor rewrites a small status file after each capacity report the
//! controller accepts, and the probe checks that file is recent.
use super::slots::{Slot, State};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A report older than this means the supervisor or its controller link is stuck.
const STALE_SECONDS: u64 = 15;

#[derive(Serialize, Deserialize)]
struct Status {
    reported_at: u64,
    /// Ready or running slots; zero while every slot is still preparing.
    usable: usize,
    draining: bool,
}

fn file(data: &Path) -> PathBuf {
    data.join("executor-health.json")
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// Record an accepted capacity report; written atomically by rename.
pub(super) fn record(data: &Path, slots: &[Slot], draining: bool) {
    let status = Status {
        reported_at: now(),
        usable: slots
            .iter()
            .filter(|slot| matches!(slot.state, State::Ready(_) | State::Running(_)))
            .count(),
        draining,
    };
    let temporary = data.join("executor-health.json.tmp");
    let written = serde_json::to_vec(&status)
        .map_err(anyhow::Error::from)
        .and_then(|bytes| Ok(std::fs::write(&temporary, bytes)?))
        .and_then(|()| Ok(std::fs::rename(&temporary, file(data))?));
    if written.is_err() {
        tracing::warn!("Could not record executor health status");
    }
}

/// A stopped executor must not look healthy from a leftover file.
pub(super) fn clear(data: &Path) {
    let _ = std::fs::remove_file(file(data));
}

/// Healthy: a recent accepted report, not draining, and at least one usable slot.
pub fn check(data: &Path) -> Result<()> {
    let bytes = std::fs::read(file(data)).context("Executor has not reported to its controller")?;
    let status: Status = serde_json::from_slice(&bytes)?;
    ensure!(
        now().saturating_sub(status.reported_at) <= STALE_SECONDS,
        "Executor has not reported to its controller in {STALE_SECONDS} seconds"
    );
    ensure!(!status.draining, "Executor is draining");
    ensure!(status.usable > 0, "Executor has no prepared slots yet");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(data: &Path, status: Status) {
        std::fs::write(file(data), serde_json::to_vec(&status).unwrap()).unwrap();
    }

    #[test]
    fn only_a_recent_report_with_usable_slots_is_healthy() {
        let data = tempfile::tempdir().unwrap();
        assert!(check(data.path()).is_err());
        let fresh = |usable, draining| Status {
            reported_at: now(),
            usable,
            draining,
        };
        write(data.path(), fresh(1, false));
        assert!(check(data.path()).is_ok());
        write(data.path(), fresh(0, false));
        assert!(check(data.path()).is_err());
        write(data.path(), fresh(2, true));
        assert!(check(data.path()).is_err());
        write(
            data.path(),
            Status {
                reported_at: now() - 60,
                usable: 2,
                draining: false,
            },
        );
        assert!(check(data.path()).is_err());
        clear(data.path());
        assert!(check(data.path()).is_err());
    }
}

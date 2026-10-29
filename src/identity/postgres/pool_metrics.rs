//! Pool and database measurements for autoscaling and capacity planning
//! (D226). Read-only; counts and sizes only, never message contents.
use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Time spent waiting for the global admission lock since the last snapshot,
/// across every caller in this process.
static ADMISSION_WAITS: AtomicU64 = AtomicU64::new(0);
static ADMISSION_TOTAL_US: AtomicU64 = AtomicU64::new(0);
static ADMISSION_MAX_US: AtomicU64 = AtomicU64::new(0);

pub(super) fn admission_waited(wait: Duration) {
    let micros = u64::try_from(wait.as_micros()).unwrap_or(u64::MAX);
    ADMISSION_WAITS.fetch_add(1, Ordering::Relaxed);
    ADMISSION_TOTAL_US.fetch_add(micros, Ordering::Relaxed);
    ADMISSION_MAX_US.fetch_max(micros, Ordering::Relaxed);
}

#[derive(Debug, Default, PartialEq)]
pub(crate) struct PoolSnapshot {
    /// Chat replies and executor tasks waiting for a slot.
    pub queued: i64,
    pub oldest_queued_seconds: i64,
    pub ready_slots: i64,
    /// Offered or running slots.
    pub busy_slots: i64,
    pub executors: i64,
    pub admission_waits: u64,
    pub admission_wait_avg_ms: f64,
    pub admission_wait_max_ms: f64,
    pub database_connections: i64,
    pub database_bytes: i64,
}

impl PgStore {
    /// One consistent read of pool and database load. Admission figures cover
    /// the interval since the previous snapshot in this process.
    pub(crate) async fn pool_snapshot(&self) -> Result<PoolSnapshot> {
        let row = query(&self.db, include_str!("pool_metrics.sql"), vec![])
            .await?
            .context("Pool metrics unavailable")?;
        let waits = ADMISSION_WAITS.swap(0, Ordering::Relaxed);
        let total = ADMISSION_TOTAL_US.swap(0, Ordering::Relaxed);
        let max = ADMISSION_MAX_US.swap(0, Ordering::Relaxed);
        Ok(PoolSnapshot {
            queued: row.try_get("", "queued")?,
            oldest_queued_seconds: row.try_get("", "oldest_queued_seconds")?,
            ready_slots: row.try_get("", "ready_slots")?,
            busy_slots: row.try_get("", "busy_slots")?,
            executors: row.try_get("", "executors")?,
            admission_waits: waits,
            admission_wait_avg_ms: if waits == 0 {
                0.0
            } else {
                total as f64 / waits as f64 / 1000.0
            },
            admission_wait_max_ms: max as f64 / 1000.0,
            database_connections: row.try_get("", "connections")?,
            database_bytes: row.try_get("", "database_bytes")?,
        })
    }

    /// The largest tables by total size (data, indexes and TOAST), for the
    /// growth rules in the database scaling plan.
    pub(crate) async fn largest_tables(&self, limit: i64) -> Result<Vec<(String, i64)>> {
        rows(&self.db,"SELECT c.relname AS name,pg_total_relation_size(c.oid) AS bytes FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
            WHERE c.relkind IN ('r','p') AND n.nspname=current_schema() ORDER BY bytes DESC LIMIT $1",vec![limit.into()]).await?
            .into_iter()
            .map(|row| Ok((row.try_get("", "name")?, row.try_get("", "bytes")?)))
            .collect()
    }
}

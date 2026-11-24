//! When the scheduler loop must look again. Schedules became tasks in schema 147;
//! task runs and follow-ups are wake-ups of tasks (task_wakeups.rs).
use super::*;
impl PgStore {
    /// How long until the scheduler next has something due: a task run or
    /// follow-up, a panel update or a helper's wait deadline; None when nothing
    /// is. Work already due but held back (its last run unfinished, a full
    /// queue) is not counted: finishing that work is announced (schema 97).
    pub(crate) async fn next_admission_in(&self) -> Result<Option<Duration>> {
        let wait = query(&self.db, "WITH now AS (SELECT (extract(epoch FROM clock_timestamp())*1000)::bigint AS ms)
            SELECT LEAST(
             (SELECT min(due_ms) FROM relay_task_wakeups,now WHERE state='pending' AND due_ms>now.ms),
             (SELECT min(due_ms) FROM relay_home_update_wanted,now WHERE due_ms>now.ms),
             (SELECT min(expires_ms) FROM relay_agent_handoffs,now WHERE state='pending' AND expires_ms>now.ms)
            )-now.ms AS wait FROM now", vec![]).await?
            .context("Due time unavailable")?
            .try_get::<Option<i64>>("", "wait")?;
        Ok(wait.map(|ms| Duration::from_millis(ms.max(0) as u64)))
    }
}

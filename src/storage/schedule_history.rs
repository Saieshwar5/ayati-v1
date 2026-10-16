use super::{Store, Task};
use anyhow::{Result, ensure};
use serde::Serialize;

#[derive(Serialize)]
pub struct ScheduleRun {
    pub due_at_ms: i64,
    pub task: Task,
}

impl Store {
    /// Read only this agent's most recent occurrences, independently of chat limits.
    pub fn schedule_runs(&self, id: &str) -> Result<Vec<ScheduleRun>> {
        let connection = self.connection()?;
        let exists: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM schedules WHERE id=?1)",
            [id],
            |r| r.get(0),
        )?;
        ensure!(exists, "Schedule does not exist");
        let mut statement = connection.prepare(
            "SELECT t.id,t.status,t.detail,t.created_at,r.due_at_ms FROM schedule_runs r JOIN tasks t ON t.id=r.task_id WHERE r.schedule_id=?1 ORDER BY r.due_at_ms DESC LIMIT 20",
        )?;
        Ok(statement
            .query_map([id], |r| {
                Ok(ScheduleRun {
                    task: Self::read_task(r)?,
                    due_at_ms: r.get(4)?,
                })
            })?
            .collect::<Result<_, _>>()?)
    }
}

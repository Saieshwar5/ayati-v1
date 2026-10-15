use super::{Store, Task, work::IntakeError};
use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalEvent {
    pub source: String,
    pub event_id: String,
    pub content: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewSchedule {
    pub id: String,
    pub content: String,
    pub run_at_ms: i64,
    pub interval_seconds: Option<i64>,
}

#[derive(Serialize)]
pub struct Schedule {
    pub id: String,
    pub content: String,
    pub run_at_ms: i64,
    pub interval_seconds: Option<i64>,
    pub next_run_ms: Option<i64>,
    pub state: String,
    pub last_task_id: Option<String>,
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

impl Store {
    pub fn dispatch_paused(&self) -> Result<bool> {
        Ok(self.connection()?.query_row(
            "SELECT paused FROM dispatch_control WHERE id=1",
            [],
            |r| r.get(0),
        )?)
    }

    pub fn resume_dispatch(&self) -> Result<()> {
        self.connection()?
            .execute("UPDATE dispatch_control SET paused=0 WHERE id=1", [])?;
        Ok(())
    }

    // This is a loopback fixture/owner boundary, not a public connector webhook.
    pub fn accept_event(&self, event: &LocalEvent) -> Result<Task> {
        Self::validate_input(&event.content)?;
        ensure!(
            !event.source.is_empty()
                && event.source.len() <= 64
                && event
                    .source
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)),
            "Event source must contain 1–64 letters, digits, dots, underscores or hyphens"
        );
        ensure!(
            !event.event_id.is_empty()
                && event.event_id.len() <= 128
                && event.event_id.bytes().all(|b| b.is_ascii_graphic()),
            "Event ID must contain 1–128 visible ASCII bytes"
        );
        let mut connection = self.connection()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let previous: Option<(String, String)> = tx.query_row("SELECT e.task_id,m.content FROM event_receipts e JOIN messages m ON m.task_id=e.task_id AND m.role='user' WHERE e.source=?1 AND e.event_id=?2", params![event.source,event.event_id], |r| Ok((r.get(0)?,r.get(1)?))).optional()?;
        let id = if let Some((id, content)) = previous {
            if content != event.content {
                return Err(IntakeError::Conflict.into());
            }
            id
        } else {
            let id = Self::accept_input_in(&tx, &event.content, &Uuid::new_v4().to_string())?;
            tx.execute(
                "INSERT INTO event_receipts(source,event_id,task_id) VALUES(?1,?2,?3)",
                params![event.source, event.event_id, id],
            )?;
            id
        };
        tx.commit()?;
        drop(connection);
        self.task(&id)?.context("Event job was not saved")
    }

    pub fn schedules(&self) -> Result<Vec<Schedule>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare("SELECT id,content,run_at_ms,interval_seconds,next_run_ms,state,last_task_id FROM schedules ORDER BY run_at_ms,id")?;
        Ok(statement
            .query_map([], |r| {
                Ok(Schedule {
                    id: r.get(0)?,
                    content: r.get(1)?,
                    run_at_ms: r.get(2)?,
                    interval_seconds: r.get(3)?,
                    next_run_ms: r.get(4)?,
                    state: r.get(5)?,
                    last_task_id: r.get(6)?,
                })
            })?
            .collect::<Result<_, _>>()?)
    }

    pub fn add_schedule(&self, input: &NewSchedule) -> Result<Schedule> {
        let id = Uuid::parse_str(&input.id)
            .context("Schedule id must be a UUID")?
            .to_string();
        Self::validate_input(&input.content)?;
        ensure!(
            (1..=253402300799000).contains(&input.run_at_ms),
            "run_at_ms must be a positive UTC Unix timestamp in milliseconds, before year 10000"
        );
        ensure!(
            input
                .interval_seconds
                .is_none_or(|n| (1..=31536000).contains(&n)),
            "interval_seconds must be 1–31536000, or omitted for one-time work"
        );
        let mut connection = self.connection()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let previous: Option<(String, i64, Option<i64>)> = tx
            .query_row(
                "SELECT content,run_at_ms,interval_seconds FROM schedules WHERE id=?1",
                [&id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        if let Some(previous) = previous {
            if previous
                != (
                    input.content.clone(),
                    input.run_at_ms,
                    input.interval_seconds,
                )
            {
                return Err(IntakeError::Conflict.into());
            }
        } else {
            let count: i64 = tx.query_row("SELECT COUNT(*) FROM schedules", [], |r| r.get(0))?;
            ensure!(
                count < 100,
                "The local proof supports at most 100 saved schedules"
            );
            tx.execute("INSERT INTO schedules(id,content,run_at_ms,interval_seconds,next_run_ms,state) VALUES(?1,?2,?3,?4,?3,'active')", params![id,input.content,input.run_at_ms,input.interval_seconds])?;
        }
        tx.commit()?;
        drop(connection);
        self.schedules()?
            .into_iter()
            .find(|s| s.id == id)
            .context("Schedule was not saved")
    }

    pub fn set_schedule_enabled(&self, id: &str, enabled: bool) -> Result<()> {
        let mut connection = self.connection()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let state: String = tx
            .query_row("SELECT state FROM schedules WHERE id=?1", [id], |r| {
                r.get(0)
            })
            .optional()?
            .context("Schedule does not exist")?;
        ensure!(
            state != "completed",
            "Completed one-time schedules cannot be restarted; create a new schedule"
        );
        tx.execute(
            "UPDATE schedules SET state=?2 WHERE id=?1",
            params![id, if enabled { "active" } else { "paused" }],
        )?;
        if !enabled {
            // Pausing prevents not-yet-started deliveries. An active job has its
            // own Stop control and can finish without replay on schedule resume.
            tx.execute("UPDATE tasks SET status='stopped',detail='Schedule paused by you' WHERE status='queued' AND id IN (SELECT task_id FROM schedule_runs WHERE schedule_id=?1)", [id])?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn dispatch_due_schedules(&self, now: i64) -> Result<Vec<String>> {
        ensure!(
            (0..=253402300799000).contains(&now),
            "Invalid scheduler clock"
        );
        let mut connection = self.connection()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let paused: bool =
            tx.query_row("SELECT paused FROM dispatch_control WHERE id=1", [], |r| {
                r.get(0)
            })?;
        if paused {
            return Ok(vec![]);
        }
        let due: Vec<(String, String, i64, Option<i64>)> = {
            let mut statement = tx.prepare("SELECT id,content,next_run_ms,interval_seconds FROM schedules s WHERE state='active' AND next_run_ms<=?1 AND NOT EXISTS(SELECT 1 FROM schedule_runs r JOIN tasks t ON t.id=r.task_id WHERE r.schedule_id=s.id AND t.status IN ('queued','running')) ORDER BY next_run_ms,id LIMIT 32")?;
            statement
                .query_map([now], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
                .collect::<Result<_, _>>()?
        };
        let mut tasks = vec![];
        for (id, content, next, interval) in due {
            // Coalesce missed fixed intervals to the latest due occurrence.
            // The same schedule never accumulates multiple pending jobs.
            let due_at = interval.map_or(next, |seconds| {
                next + (now - next) / (seconds * 1000) * (seconds * 1000)
            });
            let task = match Self::accept_input_in(&tx, &content, &Uuid::new_v4().to_string()) {
                Ok(task) => task,
                Err(error)
                    if matches!(error.downcast_ref::<IntakeError>(), Some(IntakeError::Full)) =>
                {
                    break;
                }
                Err(error) => return Err(error),
            };
            tx.execute(
                "INSERT INTO schedule_runs(schedule_id,due_at_ms,task_id) VALUES(?1,?2,?3)",
                params![id, due_at, task],
            )?;
            let next = interval.map(|seconds| due_at + seconds * 1000);
            tx.execute(
                "UPDATE schedules SET next_run_ms=?2,state=?3,last_task_id=?4 WHERE id=?1",
                params![
                    id,
                    next,
                    if next.is_some() {
                        "active"
                    } else {
                        "completed"
                    },
                    task
                ],
            )?;
            tasks.push(task);
        }
        tx.commit()?;
        Ok(tasks)
    }
}

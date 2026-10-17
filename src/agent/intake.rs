use super::Agent;
use crate::storage::{
    Task,
    intake::{LocalEvent, NewSchedule, Schedule, now_ms},
};
use anyhow::{Result, ensure};
use std::sync::{Arc, atomic::Ordering};

impl Agent {
    pub(super) fn start_intake(agent: Arc<Self>) {
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(1));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    biased;
                    _ = agent.shutdown.cancelled() => break,
                    _ = tick.tick() => {},
                }
                let result = (|| -> Result<Vec<String>> {
                    let _active = agent
                        .cancellation
                        .lock()
                        .map_err(|_| anyhow::anyhow!("Agent state unavailable"))?;
                    if agent.closing.load(Ordering::SeqCst) {
                        return Ok(vec![]);
                    }
                    if let Some(platform) = agent.platform() {
                        platform.fence_panel_schedules(agent.identity(), &agent.store)?;
                    }
                    let mut tasks = agent.store.dispatch_due_schedules(now_ms())?;
                    if let Some(platform) = agent.platform() {
                        platform.fence_trigger_jobs(agent.identity(), &agent.store)?;
                        tasks.extend(platform.dispatch_triggers(agent.identity(), &agent.store)?);
                    }
                    Ok(tasks)
                })();
                match result {
                    Ok(tasks) if !tasks.is_empty() => {
                        agent.wake.notify_one();
                        agent.emit("state", "", None, None);
                    }
                    Ok(_) => {}
                    Err(error) => {
                        tracing::error!(%error, "Scheduled intake failed; due work remains saved")
                    }
                }
            }
        });
    }

    fn ensure_intake(&self) -> Result<()> {
        ensure!(
            self.goose.is_some(),
            "Schedules/events require the Goose runtime"
        );
        ensure!(
            !self.closing.load(Ordering::SeqCst),
            "Ayati is stopping; retry after restart"
        );
        Ok(())
    }

    pub fn accept_event(&self, event: &LocalEvent) -> Result<Task> {
        let _active = self
            .cancellation
            .lock()
            .map_err(|_| anyhow::anyhow!("Agent state unavailable"))?;
        self.ensure_intake()?;
        let task = self.store.accept_event(event)?;
        self.wake.notify_one();
        self.emit("state", &task.id, None, None);
        Ok(task)
    }

    pub fn add_schedule(&self, schedule: &NewSchedule) -> Result<Schedule> {
        let _active = self
            .cancellation
            .lock()
            .map_err(|_| anyhow::anyhow!("Agent state unavailable"))?;
        self.ensure_intake()?;
        let saved = self.store.add_schedule(schedule)?;
        self.emit("state", "", None, None);
        Ok(saved)
    }

    pub fn set_schedule_enabled(&self, id: &str, enabled: bool) -> Result<()> {
        let _active = self
            .cancellation
            .lock()
            .map_err(|_| anyhow::anyhow!("Agent state unavailable"))?;
        self.ensure_intake()?;
        self.store.set_schedule_enabled(id, enabled)?;
        self.wake.notify_one();
        self.emit("state", "", None, None);
        Ok(())
    }
}

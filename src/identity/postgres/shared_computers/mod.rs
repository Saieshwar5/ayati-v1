//! Your own computer (schema 119, B3 of agent building blocks, D248). A
//! workspace adds a machine it runs itself; the Cuckoo Connector on it signs in
//! once with a one-time code, then connects outward to take work, so no ports
//! are opened. Admins choose which agents use it. Each agent uses one computer:
//! none, Cuckoo's or one of these. One request holds the machine at a time,
//! and each agent works in its own folder there. Commands, files and the
//! Programs rule are the same as on Cuckoo's computers; the browser stays on
//! Cuckoo's computers for now.
use super::*;
use crate::identity::chat::model::Job;
use serde_json::json;
mod connector;
mod manage;
pub(crate) mod relay;
#[cfg(test)]
mod tests;
pub(crate) use connector::Refused;
pub(crate) use migration::SharedComputers;

/// A connector not heard from for this long is offline.
pub(crate) const ONLINE_MS: i64 = 90_000;
/// A request keeps the machine this long after its last use; others wait.
const HOLD_SECONDS: i64 = 180;
/// How long a request waits for its turn before it is told to try again.
#[cfg(not(test))]
const WAIT: std::time::Duration = std::time::Duration::from_secs(30);
#[cfg(test)]
const WAIT: std::time::Duration = std::time::Duration::from_secs(3);

/// The workspace computer an agent uses.
#[derive(Clone, Debug)]
pub(crate) struct Assigned {
    pub computer: String,
    pub name: String,
}

pub(crate) async fn ready(db: &impl ConnectionTrait) -> Result<bool> {
    Ok(query(
        db,
        "SELECT to_regclass('relay_workspace_computers') IS NOT NULL AS ready",
        vec![],
    )
    .await?
    .context("Missing schema check")?
    .try_get("", "ready")?)
}

/// The workspace computer this agent uses, if any.
pub(crate) async fn assigned(
    db: &impl ConnectionTrait,
    w: &str,
    agent: &str,
) -> Result<Option<Assigned>> {
    if !ready(db).await? {
        return Ok(None);
    }
    query(db, "SELECT c.id,c.name FROM relay_workspace_computer_agents a JOIN relay_workspace_computers c ON c.workspace=a.workspace AND c.id=a.computer
        WHERE a.workspace=$1 AND a.agent=$2 AND NOT c.removed", vec![w.into(), agent.into()]).await?
        .map(|r| Ok(Assigned { computer: r.try_get("", "id")?, name: r.try_get("", "name")? }))
        .transpose()
}

impl PgStore {
    /// Whether the agent has a computer it can use now: its workspace's, or
    /// Cuckoo's when computers are set up and its own is switched on.
    pub(super) async fn computer_usable(
        &self,
        job: &Job,
        settings: &crate::identity::agents::computer_settings::ComputerSettings,
    ) -> Result<bool> {
        Ok(assigned(&self.db, &job.workspace, &job.agent)
            .await?
            .is_some()
            || (self.computers.is_some() && settings.on))
    }

    /// Takes the workspace computer for this request, waiting while another
    /// request holds it, or says why it can't be used now.
    pub(super) async fn shared_turn(
        &self,
        job: &Job,
        computer: &Assigned,
    ) -> Result<std::result::Result<(), Value>> {
        let started = std::time::Instant::now();
        loop {
            let row = query(&self.db, "SELECT device IS NOT NULL AS connected,COALESCE(last_seen_ms,0) AS seen,(extract(epoch FROM clock_timestamp())*1000)::bigint AS now
                FROM relay_workspace_computers WHERE workspace=$1 AND id=$2 AND NOT removed",
                vec![job.workspace.clone().into(), computer.computer.clone().into()]).await?;
            let Some(row) = row else {
                return Ok(Err(super::computers::refused(
                    "Your workspace's computer was removed. Answer without it.",
                )));
            };
            if !row.try_get::<bool>("", "connected")? {
                return Ok(Err(super::computers::unavailable(&format!(
                    "{} isn't connected yet: an admin needs to start the Cuckoo Connector on it. Answer without it.",
                    computer.name
                ))));
            }
            if row.try_get::<i64>("", "now")? - row.try_get::<i64>("", "seen")? > ONLINE_MS {
                return Ok(Err(super::computers::unavailable(&format!(
                    "{} is offline: it may be switched off or asleep. Try again later, or answer without it.",
                    computer.name
                ))));
            }
            let at = now();
            let taken = execute(&self.db, "UPDATE relay_workspace_computers SET held_by=$3,held_until=$4 WHERE workspace=$1 AND id=$2 AND (held_by IS NULL OR held_by=$3 OR held_until<$5)",
                vec![job.workspace.clone().into(), computer.computer.clone().into(), job.reply.clone().into(), (at + HOLD_SECONDS).into(), at.into()]).await?;
            if taken == 1 {
                return Ok(Ok(()));
            }
            if started.elapsed() > WAIT {
                return Ok(Err(super::computers::unavailable(&format!(
                    "{} is busy with another request. Try again in a minute, or answer without it.",
                    computer.name
                ))));
            }
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        }
    }

    /// Keeps the workspace computer held a while after a use.
    pub(super) async fn shared_used(&self, job: &Job) -> Result<()> {
        if !ready(&self.db).await? {
            return Ok(());
        }
        execute(
            &self.db,
            "UPDATE relay_workspace_computers SET held_until=$3 WHERE workspace=$1 AND held_by=$2",
            vec![
                job.workspace.clone().into(),
                job.reply.clone().into(),
                (now() + HOLD_SECONDS).into(),
            ],
        )
        .await?;
        Ok(())
    }
}

/// Lets other requests use the workspace computer once this one finishes.
pub(super) async fn release(db: &impl ConnectionTrait, w: &str, reply: &str) -> Result<()> {
    if ready(db).await? {
        execute(db, "UPDATE relay_workspace_computers SET held_by=NULL,held_until=0 WHERE workspace=$1 AND held_by=$2",
            vec![w.into(), reply.into()]).await?;
    }
    Ok(())
}

/// What an admin sees for one computer, never its keys.
pub(super) fn state(row: &Value, now_ms: i64) -> &'static str {
    if row["connected"] != true {
        "waiting"
    } else if now_ms - row["last_seen_ms"].as_i64().unwrap_or(0) > ONLINE_MS {
        "offline"
    } else {
        "online"
    }
}

pub(super) fn digest(value: &str) -> String {
    crate::identity::provider::digest(value)
}

pub(super) fn same(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

pub(super) fn listing(rows: Vec<Value>, now_ms: i64) -> Vec<Value> {
    rows.into_iter()
        .map(|mut row| {
            row["state"] = json!(state(&row, now_ms));
            row
        })
        .collect()
}

mod migration {
    use sea_orm_migration::prelude::*;
    pub(crate) struct SharedComputers;
    impl MigrationName for SharedComputers {
        fn name(&self) -> &str {
            "m20261114_000119_workspace_computers"
        }
    }
    #[async_trait::async_trait]
    impl MigrationTrait for SharedComputers {
        async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
            m.get_connection()
                .execute_unprepared(include_str!("shared_computers.sql"))
                .await?;
            Ok(())
        }
        async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
            Err(DbErr::Custom(
                "Workspace computers require a forward migration".into(),
            ))
        }
    }
}

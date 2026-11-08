//! Taking turns on an agent's computer (schema 106): one request holds it at a time and
//! others wait in line, oldest first, instead of being turned away; a workspace may
//! also limit how many computers are awake at once. Hand-overs keep a picture of the
//! page the person is asked to help with, and operators can pause every computer.
//! See runtime/agent-computers.md.
use super::*;
use crate::identity::chat::model::Job;
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};
use std::time::{Duration, Instant};

/// How long a request waits for its turn before it is told to try again.
#[cfg(not(test))]
const WAIT: Duration = Duration::from_secs(30);
#[cfg(test)]
const WAIT: Duration = Duration::from_secs(4);
/// A waiting request that hasn't asked again for this long has left the line.
const GONE_SECONDS: i64 = 120;

pub(super) struct ComputerTurnsMigration;
impl MigrationName for ComputerTurnsMigration {
    fn name(&self) -> &str {
        "m20261108_000106_computer_turns"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for ComputerTurnsMigration {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(
                "CREATE TABLE relay_computer_waits (
                  workspace TEXT NOT NULL, agent TEXT NOT NULL, reply TEXT NOT NULL,
                  since BIGINT NOT NULL, seen BIGINT NOT NULL,
                  PRIMARY KEY(workspace, agent, reply));
                 CREATE TABLE relay_computer_shots (
                  workspace TEXT NOT NULL, reply TEXT NOT NULL, key TEXT NOT NULL,
                  at BIGINT NOT NULL,
                  image BYTEA NOT NULL CHECK(octet_length(image) <= 600000),
                  PRIMARY KEY(workspace, reply, key),
                  FOREIGN KEY(workspace, reply) REFERENCES relay_chat_messages(workspace, id) ON DELETE CASCADE);
                 -- Operators can pause every agent computer (Cuckoo Control).
                 ALTER TABLE relay_platform_settings DROP CONSTRAINT relay_platform_settings_key_check;
                 ALTER TABLE relay_platform_settings ADD CONSTRAINT relay_platform_settings_key_check
                   CHECK(key IN ('signup.paused','platform.daily_ceiling_usd','computers.paused'));",
            )
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Computer turns require a forward migration".into(),
        ))
    }
}

/// The computer's row after this request took its turn, or why it can't now.
pub(super) enum Turn {
    Taken(QueryResult),
    Refused(Value),
}

impl PgStore {
    /// Takes the agent's computer for this request, waiting in line while another
    /// request holds it or the workspace's limit of awake computers is reached.
    pub(super) async fn computer_turn(
        &self,
        job: &Job,
        size: &str,
        provider: &str,
        image: &str,
        max_awake: u32,
        hold_seconds: i64,
    ) -> Result<Turn> {
        let started = Instant::now();
        let (w, agent, reply) = (&job.workspace, &job.agent, &job.reply);
        loop {
            let at = now();
            let tx = self.db.begin().await?;
            // A computer not made yet follows the size chosen now; one already made keeps
            // its size until it is reset.
            execute(&tx, "INSERT INTO relay_computers(workspace,agent,size,provider,image,created_at) VALUES($1,$2,$3,$4,$5,$6)
                ON CONFLICT(workspace,agent) DO UPDATE SET size=EXCLUDED.size,provider=EXCLUDED.provider,image=EXCLUDED.image
                WHERE relay_computers.provider_id IS NULL AND relay_computers.state='asleep'",
                vec![w.into(), agent.into(), size.into(), provider.into(), image.into(), at.into()]).await?;
            let row = query(&tx, "SELECT size,provider,image,provider_id,state,internet,held_by,held_until FROM relay_computers WHERE workspace=$1 AND agent=$2 FOR UPDATE",
                vec![w.into(), agent.into()]).await?.context("Computer unavailable")?;
            let state: String = row.try_get("", "state")?;
            if state == "deleting" {
                return Ok(Turn::Refused(super::computers::unavailable(
                    "Your computer is being reset. Try again in a minute.",
                )));
            }
            let held_by: Option<String> = row.try_get("", "held_by")?;
            let held_until: i64 = row.try_get("", "held_until")?;
            let busy = held_by.as_deref().is_some_and(|r| r != reply.as_str()) && held_until > at;
            execute(
                &tx,
                "DELETE FROM relay_computer_waits WHERE workspace=$1 AND agent=$2 AND seen<$3",
                vec![w.into(), agent.into(), (at - GONE_SECONDS).into()],
            )
            .await?;
            let ahead: i64 = query(&tx, "SELECT count(*)::bigint AS ahead FROM relay_computer_waits o WHERE o.workspace=$1 AND o.agent=$2 AND o.reply<>$3
                  AND o.since < COALESCE((SELECT since FROM relay_computer_waits WHERE workspace=$1 AND agent=$2 AND reply=$3), $4+1)",
                vec![w.into(), agent.into(), reply.into(), at.into()]).await?.context("Line unavailable")?.try_get("", "ahead")?;
            let full = state != "awake"
                && max_awake > 0
                && {
                    let awake: i64 = query(&tx, "SELECT count(*)::bigint AS awake FROM relay_computers WHERE workspace=$1 AND agent<>$2 AND state='awake'",
                    vec![w.into(), agent.into()]).await?.context("Count unavailable")?.try_get("", "awake")?;
                    awake >= i64::from(max_awake)
                };
            if !busy && ahead == 0 && !full {
                execute(
                    &tx,
                    "DELETE FROM relay_computer_waits WHERE workspace=$1 AND agent=$2 AND reply=$3",
                    vec![w.into(), agent.into(), reply.into()],
                )
                .await?;
                execute(&tx, "UPDATE relay_computers SET held_by=$3,held_until=$4,last_used=$5 WHERE workspace=$1 AND agent=$2",
                    vec![w.into(), agent.into(), reply.into(), (at + hold_seconds).into(), at.into()]).await?;
                tx.commit().await?;
                return Ok(Turn::Taken(row));
            }
            execute(&tx, "INSERT INTO relay_computer_waits(workspace,agent,reply,since,seen) VALUES($1,$2,$3,$4,$4)
                ON CONFLICT(workspace,agent,reply) DO UPDATE SET seen=EXCLUDED.seen",
                vec![w.into(), agent.into(), reply.into(), at.into()]).await?;
            tx.commit().await?;
            if started.elapsed() >= WAIT {
                return Ok(Turn::Refused(super::computers::unavailable(
                    &if full && !busy {
                        format!(
                            "Too many computers are working in this workspace right now (the limit is {max_awake}). You keep your place; try again in a minute, or answer without it."
                        )
                    } else {
                        let place = ahead + 1;
                        format!(
                            "Your computer is busy with another request; you are number {place} in line and keep your place. Try again in a minute, or answer without it."
                        )
                    },
                )));
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }

    /// Keeps a picture of the page a person is asked to help with in a hand-over.
    pub(super) async fn computer_shot(&self, job: &Job, key: &str, image: &[u8]) -> Result<()> {
        execute(
            &self.db,
            "INSERT INTO relay_computer_shots(workspace,reply,key,at,image) VALUES($1,$2,$3,$4,$5)
            ON CONFLICT(workspace,reply,key) DO UPDATE SET at=EXCLUDED.at,image=EXCLUDED.image",
            vec![
                job.workspace.clone().into(),
                job.reply.clone().into(),
                key.into(),
                now().into(),
                image.to_vec().into(),
            ],
        )
        .await?;
        Ok(())
    }
}

/// The hand-over's picture, as a data address for the question view.
pub(super) async fn hand_over_shot(
    db: &impl ConnectionTrait,
    w: &str,
    reply: &str,
    key: &str,
) -> Result<Option<String>> {
    if !key.starts_with("browser-hand-over-")
        || !super::model_budgets::present(db, "relay_computer_shots").await?
    {
        return Ok(None);
    }
    let row = query(
        db,
        "SELECT image FROM relay_computer_shots WHERE workspace=$1 AND reply=$2 AND key=$3",
        vec![w.into(), reply.into(), key.into()],
    )
    .await?;
    use base64::Engine as _;
    Ok(row
        .map(|r| r.try_get::<Vec<u8>>("", "image"))
        .transpose()?
        .map(|image| {
            format!(
                "data:image/jpeg;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(image)
            )
        }))
}

//! Provider operations (schema 110): every gateway call through a connector
//! provider is recorded with its latency, outcome and estimated cost; before a
//! call, Cuckoo refuses it when the provider's circuit breaker is open or the
//! workspace called the app too often in the last minute. Both live in
//! PostgreSQL, so every controller agrees. See runtime/connector-providers.md.
use super::control_actions::Dispatch;
use super::*;
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};
use std::time::Instant;
#[path = "connector_drift.rs"]
pub(crate) mod drift;
#[cfg(test)]
#[path = "connector_drift_tests.rs"]
mod drift_tests;
#[cfg(test)]
#[path = "connector_operations_tests.rs"]
mod tests;

/// Calls one workspace may make to one app per minute.
pub(crate) const RATE_PER_MINUTE: i64 = 60;
const RATE_WINDOW_MS: i64 = 60_000;
/// Provider failures in a row, within the window, that open the breaker.
pub(crate) const BREAKER_FAILURES: i32 = 5;
const BREAKER_WINDOW_MS: i64 = 120_000;
/// How long an open breaker refuses calls before one may test the provider.
pub(crate) const BREAKER_COOLDOWN_MS: i64 = 180_000;
/// Longest a trial call holds its turn: preparing (30 s) plus sending (45 s).
const PROBE_LEASE_MS: i64 = 90_000;

pub(crate) const BREAKER_REASON: &str = "Cuckoo paused calls to this service for a few minutes after repeated failures. Nothing was sent.";
pub(crate) const RATE_REASON: &str = "This workspace made too many calls to this app in the last minute. Nothing was sent; try again in a minute.";
const NOW_MS: &str = "(extract(epoch FROM clock_timestamp())*1000)::bigint";

pub(crate) struct ConnectorOperations;
impl MigrationName for ConnectorOperations {
    fn name(&self) -> &str {
        "m20261108_000110_connector_operations"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for ConnectorOperations {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("connector_operations.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Connector operations require a forward migration".into(),
        ))
    }
}

/// A call let through to its provider; [`PgStore::connector_call_ended`]
/// records how it ended.
pub(crate) struct CallRecord {
    id: String,
    provider: String,
    started: Instant,
}

pub(crate) enum Admission {
    Admitted(CallRecord),
    /// Refused before anything reached the provider, with the reason people see.
    Refused(&'static str),
}

/// How an admitted call ended.
pub(crate) enum Ended {
    Completed,
    /// Sent, but the provider did not confirm the result (or did not answer).
    Uncertain,
    /// Stopped before sending. `provider_error`: the provider failed or did not
    /// answer the account and action check; a refusal it explained is not one.
    PreflightFailed {
        provider_error: bool,
    },
}

impl PgStore {
    /// The rate limit, then the breaker; a call let through is recorded as
    /// running. Calls Cuckoo refuses are recorded too, but never count toward
    /// the limit, so a busy agent can't keep its own app locked.
    pub(crate) async fn connector_admit(
        &self,
        w: &str,
        dispatch: &Dispatch,
        provider: &str,
    ) -> Result<Admission> {
        let tx = self.db.begin().await?;
        execute(
            &tx,
            "SELECT pg_advisory_xact_lock(hashtext('relay_connector_calls:' || $1 || ':' || $2))",
            vec![w.into(), dispatch.server.clone().into()],
        )
        .await?;
        let now: i64 = query(&tx, &format!("SELECT {NOW_MS} AS now"), vec![])
            .await?
            .context("Clock")?
            .try_get("", "now")?;
        let recent: i64 = query(&tx, "SELECT count(*) AS n FROM relay_connector_calls WHERE workspace=$1 AND app=$2 AND started_ms>$3 AND outcome IS DISTINCT FROM 'refused'",
            vec![w.into(), dispatch.server.clone().into(), (now - RATE_WINDOW_MS).into()])
            .await?
            .context("Count")?
            .try_get("", "n")?;
        let id = uuid::Uuid::new_v4().to_string();
        let refused = if recent >= RATE_PER_MINUTE {
            Some("rate_limit")
        } else if breaker_lets_through(&tx, provider, &id, now).await? {
            None
        } else {
            Some("breaker")
        };
        insert(&tx, &id, w, dispatch, provider, now, refused).await?;
        tx.commit().await?;
        Ok(match refused {
            None => Admission::Admitted(CallRecord {
                id,
                provider: provider.to_owned(),
                started: Instant::now(),
            }),
            Some("rate_limit") => Admission::Refused(RATE_REASON),
            Some(_) => Admission::Refused(BREAKER_REASON),
        })
    }

    /// A call Cuckoo's switch refused before admission.
    pub(crate) async fn connector_call_paused(
        &self,
        w: &str,
        dispatch: &Dispatch,
        provider: &str,
    ) -> Result<()> {
        let id = uuid::Uuid::new_v4().to_string();
        let now: i64 = query(&self.db, &format!("SELECT {NOW_MS} AS now"), vec![])
            .await?
            .context("Clock")?
            .try_get("", "now")?;
        insert(&self.db, &id, w, dispatch, provider, now, Some("paused")).await
    }

    /// Records how an admitted call ended and tells the provider's breaker.
    pub(crate) async fn connector_call_ended(
        &self,
        record: &CallRecord,
        ended: Ended,
    ) -> Result<()> {
        let (outcome, provider_error, failed) = match ended {
            Ended::Completed => ("completed", false, false),
            Ended::Uncertain => ("uncertain", false, true),
            Ended::PreflightFailed { provider_error } => {
                ("preflight_failed", provider_error, provider_error)
            }
        };
        let usd = if matches!(outcome, "completed" | "uncertain") {
            crate::identity::connectors::pricing::usd_per_call(&record.provider)
        } else {
            0.0
        };
        let latency = record.started.elapsed().as_millis() as i64;
        execute(&self.db, "UPDATE relay_connector_calls SET outcome=$2,provider_error=$3,latency_ms=$4,usd_estimate=$5 WHERE id=$1 AND outcome IS NULL",
            vec![record.id.clone().into(), outcome.into(), provider_error.into(), latency.into(), usd.into()]).await?;
        if failed {
            self.breaker_failed(record).await
        } else {
            // Only a write when there is something to reset: the trial call
            // closes the breaker, and a success ends a run of failures.
            execute(
                &self.db,
                &format!(
                    "UPDATE relay_connector_breakers SET failures=0,window_ms=0,
                open_until_ms=CASE WHEN probe=$2 THEN 0 ELSE open_until_ms END,
                probe_until_ms=CASE WHEN probe=$2 THEN 0 ELSE probe_until_ms END,
                probe=CASE WHEN probe=$2 THEN NULL ELSE probe END,changed_ms={NOW_MS}
                WHERE provider=$1 AND (probe=$2 OR (open_until_ms=0 AND failures>0))"
                ),
                vec![record.provider.clone().into(), record.id.clone().into()],
            )
            .await?;
            Ok(())
        }
    }

    async fn breaker_failed(&self, record: &CallRecord) -> Result<()> {
        let tx = self.db.begin().await?;
        execute(&tx, &format!("INSERT INTO relay_connector_breakers(provider,changed_ms) VALUES($1,{NOW_MS}) ON CONFLICT DO NOTHING"),
            vec![record.provider.clone().into()]).await?;
        let row = query(&tx, &format!("SELECT failures,window_ms,open_until_ms,probe,{NOW_MS} AS now FROM relay_connector_breakers WHERE provider=$1 FOR UPDATE"),
            vec![record.provider.clone().into()]).await?.context("Breaker")?;
        let (failures, window, open_until, probe, now): (i32, i64, i64, Option<String>, i64) = (
            row.try_get("", "failures")?,
            row.try_get("", "window_ms")?,
            row.try_get("", "open_until_ms")?,
            row.try_get("", "probe")?,
            row.try_get("", "now")?,
        );
        let testing =
            probe.as_deref() == Some(record.id.as_str()) || (open_until > 0 && open_until <= now);
        let (failures, window, open) = if testing {
            // The trial failed: pause again.
            (BREAKER_FAILURES, now, true)
        } else if open_until > now {
            // Already open; calls sent before it opened change nothing.
            (failures, window, false)
        } else if window + BREAKER_WINDOW_MS < now {
            (1, now, BREAKER_FAILURES <= 1)
        } else {
            (failures + 1, window, failures + 1 >= BREAKER_FAILURES)
        };
        if open {
            execute(&tx, "UPDATE relay_connector_breakers SET failures=$2,window_ms=$3,open_until_ms=$4,opened_ms=$5,probe=NULL,probe_until_ms=0,changed_ms=$5 WHERE provider=$1",
                vec![record.provider.clone().into(), failures.into(), window.into(), (now + BREAKER_COOLDOWN_MS).into(), now.into()]).await?;
            tracing::warn!(
                provider = %record.provider,
                "Paused calls to a connector provider after repeated failures"
            );
        } else {
            execute(&tx, "UPDATE relay_connector_breakers SET failures=$2,window_ms=$3,changed_ms=$4 WHERE provider=$1",
                vec![record.provider.clone().into(), failures.into(), window.into(), now.into()]).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Each provider's breaker for Cuckoo Control: closed, open (refusing) or
    /// testing (one trial call may go).
    pub(crate) async fn connector_breakers(&self) -> Result<Vec<Value>> {
        json_rows(&self.db, &format!("SELECT jsonb_build_object('provider',provider,'failures',failures,'opened_ms',opened_ms,
            'until_ms',CASE WHEN open_until_ms>{NOW_MS} THEN open_until_ms END,
            'state',CASE WHEN open_until_ms=0 THEN 'closed' WHEN open_until_ms>{NOW_MS} THEN 'open' ELSE 'testing' END) AS data
            FROM relay_connector_breakers ORDER BY provider"), vec![]).await
    }
}

/// Closed: every call goes. Open: none. Testing: one trial call at a time.
async fn breaker_lets_through(
    tx: &DatabaseTransaction,
    provider: &str,
    id: &str,
    now: i64,
) -> Result<bool> {
    let Some(row) = query(
        tx,
        "SELECT open_until_ms FROM relay_connector_breakers WHERE provider=$1",
        vec![provider.into()],
    )
    .await?
    else {
        return Ok(true);
    };
    let open_until: i64 = row.try_get("", "open_until_ms")?;
    if open_until == 0 {
        return Ok(true);
    }
    if open_until > now {
        return Ok(false);
    }
    // The cooldown passed: this call tests the provider, unless another does.
    Ok(execute(tx, "UPDATE relay_connector_breakers SET probe=$2,probe_until_ms=$3+$4,changed_ms=$3 WHERE provider=$1 AND open_until_ms>0 AND open_until_ms<=$3 AND probe_until_ms<=$3",
        vec![provider.into(), id.into(), now.into(), PROBE_LEASE_MS.into()]).await? == 1)
}

async fn insert(
    db: &impl ConnectionTrait,
    id: &str,
    w: &str,
    dispatch: &Dispatch,
    provider: &str,
    now: i64,
    refused: Option<&str>,
) -> Result<()> {
    let n = execute(db, "INSERT INTO relay_connector_calls(id,workspace,action,agent,connection,app,provider,operation,started_ms,outcome,refused,latency_ms)
        SELECT $1,a.workspace,a.id,a.agent,a.connection,$4,$5,a.operation,$6,CASE WHEN $7::text IS NULL THEN NULL ELSE 'refused' END,$7,CASE WHEN $7::text IS NULL THEN NULL ELSE 0 END
        FROM relay_control_actions a WHERE a.workspace=$2 AND a.id=$3",
        vec![id.into(), w.into(), dispatch.id.clone().into(), dispatch.server.clone().into(), provider.into(), now.into(), refused.map(str::to_owned).into()]).await?;
    ensure!(n == 1, "The action to record is missing");
    Ok(())
}

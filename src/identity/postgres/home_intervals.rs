//! A panel's fixed interval ("update every N", schema 146) runs on the panel's own
//! update queue (`relay_home_update_wanted`, reason `interval`), not as a schedule.
//! Each update run that starts books the next interval update one interval later;
//! an interval update that comes due while an update still runs waits for it.
use super::super::*;
use crate::identity::home_panels::maintenance::Agreement;

#[path = "home_intervals_migration.rs"]
pub(in crate::identity::postgres) mod migration;
#[cfg(test)]
#[path = "home_intervals_tests.rs"]
mod tests;

/// Upgrade tests run current code on older schemas, where the queue has no
/// `interval` reason yet; there an interval books nothing.
pub(in crate::identity::postgres) async fn ready(db: &impl ConnectionTrait) -> Result<bool> {
    Ok(query(
        db,
        "SELECT EXISTS(SELECT 1 FROM pg_constraint WHERE conname='relay_home_update_wanted_reason'
          AND conrelid=to_regclass('relay_home_update_wanted')) AS ready",
        vec![],
    )
    .await?
    .is_some_and(|r| r.try_get::<bool>("", "ready").unwrap_or(false)))
}

/// Want the panel's next interval update one interval from now. With `replace`, a
/// pending interval update moves to that time; otherwise an earlier one stays. Work
/// or event updates already wanted stay too: they book the interval when they start.
pub(in crate::identity::postgres) async fn book(
    db: &impl ConnectionTrait,
    w: &str,
    panel: &str,
    replace: bool,
) -> Result<()> {
    if !ready(db).await? {
        return Ok(());
    }
    execute(
        db,
        "INSERT INTO relay_home_update_wanted(workspace,panel,reason,cause,due_ms)
          SELECT workspace,panel,'interval','interval',(extract(epoch FROM clock_timestamp())*1000)::bigint+interval_seconds*1000
          FROM relay_home_maintenance WHERE workspace=$1 AND panel=$2 AND enabled AND interval_seconds IS NOT NULL
         ON CONFLICT(workspace,panel) DO UPDATE SET due_ms=excluded.due_ms,detail=''
          WHERE relay_home_update_wanted.reason='interval' AND $3::boolean",
        vec![w.into(), panel.into(), replace.into()],
    )
    .await?;
    Ok(())
}

/// The agreement was saved. A changed interval, a pause or a resume starts the
/// interval again from now; an unchanged one keeps its next update.
pub(in crate::identity::postgres) async fn agreed(
    tx: &DatabaseTransaction,
    w: &str,
    panel: &str,
    previous: &Value,
    input: &Agreement,
) -> Result<()> {
    if !ready(tx).await? {
        return Ok(());
    }
    let kept = input.enabled
        && previous["enabled"] == true
        && previous["interval_seconds"].as_i64() == input.interval_seconds;
    if !kept {
        execute(
            tx,
            "DELETE FROM relay_home_update_wanted WHERE workspace=$1 AND panel=$2 AND reason='interval'",
            vec![w.into(), panel.into()],
        )
        .await?;
    }
    book(tx, w, panel, false).await
}

/// A work or event update held back by the panel's gap: when the next interval
/// update comes sooner than the gap's end, it becomes that interval update instead,
/// which no gap holds back. Returns the time and reason it now waits for.
pub(in crate::identity::postgres) async fn within_gap(
    db: &impl ConnectionTrait,
    w: &str,
    panel: &str,
    last: i64,
    gap_end: i64,
) -> Result<(i64, &'static str)> {
    if !ready(db).await? {
        return Ok((gap_end, ""));
    }
    let interval = query(
        db,
        "SELECT interval_seconds FROM relay_home_maintenance WHERE workspace=$1 AND panel=$2",
        vec![w.into(), panel.into()],
    )
    .await?
    .and_then(|r| {
        r.try_get::<Option<i64>>("", "interval_seconds")
            .ok()
            .flatten()
    });
    Ok(match interval {
        Some(seconds) if last + seconds * 1000 < gap_end => (last + seconds * 1000, "interval"),
        _ => (gap_end, ""),
    })
}

/// When the panel's next fixed-interval update is due, for Home.
pub(in crate::identity::postgres) async fn next_ms(
    db: &impl ConnectionTrait,
    w: &str,
    panel: &str,
) -> Result<Option<i64>> {
    if !ready(db).await? {
        return Ok(None);
    }
    Ok(query(
        db,
        "SELECT k.due_ms FROM relay_home_update_wanted k JOIN relay_home_maintenance u ON u.workspace=k.workspace AND u.panel=k.panel
          WHERE k.workspace=$1 AND k.panel=$2 AND u.enabled AND u.interval_seconds IS NOT NULL",
        vec![w.into(), panel.into()],
    )
    .await?
    .map(|r| r.try_get::<i64>("", "due_ms"))
    .transpose()?)
}

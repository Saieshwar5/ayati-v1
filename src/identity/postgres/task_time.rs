//! Local dates and times of tasks (schema 144): the workspace's time zone (India
//! unless an admin chose another), the dates and times an agent gives in its own
//! words checked before PostgreSQL reads them, and run times as people read them.
use super::super::*;

fn digits(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit())
}

/// "YYYY-MM-DD", checked before PostgreSQL reads it.
fn date_shape(text: &str) -> bool {
    let parts: Vec<&str> = text.split('-').collect();
    matches!(parts.as_slice(), [y, m, d] if y.len() == 4 && m.len() == 2 && d.len() == 2 && digits(y) && digits(m) && digits(d))
}

/// The workspace's time zone, India unless an admin chose another.
pub(crate) async fn workspace_zone(db: &impl ConnectionTrait, w: &str) -> Result<String> {
    Ok(query(
        db,
        "SELECT COALESCE(timezone,'Asia/Kolkata') AS zone FROM relay_workspaces WHERE id=$1",
        vec![w.into()],
    )
    .await?
    .context("Workspace unavailable")?
    .try_get("", "zone")?)
}

/// Read a date PostgreSQL may refuse (30 February) in a savepoint, so a refusal
/// leaves the transaction usable; `None` when it is not a real date.
async fn checked(
    tx: &DatabaseTransaction,
    sql: &str,
    text: &str,
    zone: &str,
) -> Result<Option<i64>> {
    let point = tx.begin().await?;
    match query(&point, sql, vec![text.into(), zone.into()]).await {
        Ok(row) => {
            let ms = row
                .map(|r| r.try_get::<Option<i64>>("", "ms"))
                .transpose()?
                .flatten();
            point.commit().await?;
            Ok(ms)
        }
        Err(_) => {
            point.rollback().await?;
            Ok(None)
        }
    }
}

/// A local date and time in `zone` as milliseconds.
pub(super) async fn local_ms(tx: &DatabaseTransaction, text: &str, zone: &str) -> Result<i64> {
    let text = text.trim().replacen('T', " ", 1);
    let ok = text.split_once(' ').is_some_and(|(day, time)| {
        date_shape(day)
            && time.len() == 5
            && time.as_bytes()[2] == b':'
            && digits(&time[..2])
            && digits(&time[3..])
    });
    ensure!(
        ok,
        "first_at is a local date and time like 2026-11-26 10:00"
    );
    let ms = checked(tx, "SELECT CASE WHEN to_char($1::timestamp,'YYYY-MM-DD HH24:MI')=$1 THEN (extract(epoch FROM ($1::timestamp AT TIME ZONE $2))*1000)::bigint END AS ms", &text, zone).await?;
    ms.with_context(|| format!("{text} is not a real date and time"))
}

/// The end of local date `text` in `zone`, as milliseconds.
pub(super) async fn end_of_day_ms(tx: &DatabaseTransaction, text: &str, zone: &str) -> Result<i64> {
    let text = text.trim();
    ensure!(date_shape(text), "until is a local date like 2026-12-12");
    let ms = checked(tx, "SELECT CASE WHEN to_char($1::date,'YYYY-MM-DD')=$1 THEN (extract(epoch FROM (($1::date+1)::timestamp AT TIME ZONE $2))*1000)::bigint-1 END AS ms", text, zone).await?;
    ms.with_context(|| format!("{text} is not a real date"))
}

/// A run time as the person reads it: "Thu 26 Nov, 10:00".
pub(crate) async fn shown(db: &impl ConnectionTrait, ms: i64, zone: &str) -> Result<String> {
    Ok(query(
        db,
        "SELECT to_char(to_timestamp($1/1000.0) AT TIME ZONE $2,'Dy DD Mon, HH24:MI') AS shown",
        vec![ms.into(), zone.into()],
    )
    .await?
    .context("Clock unavailable")?
    .try_get("", "shown")?)
}

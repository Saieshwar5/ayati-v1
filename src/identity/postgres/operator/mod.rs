//! Cuckoo Control (schema 87): what platform operators see and change across
//! every workspace. Callers have already checked that the person is a platform
//! administrator (`AYATI_PLATFORM_ADMINS`); the web console and `ayati admin`
//! share these functions, so both write the same change log.
//!
//! Operators see counts, names, settings and spend. Nothing here reads message
//! contents or Library files.
use super::*;
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};
use serde_json::json;

pub(crate) mod log;
pub(crate) mod models;
pub(crate) mod overview;
pub(crate) mod providers;
#[cfg(test)]
mod routing_tests;
pub(crate) mod settings;
pub(crate) mod spend;
#[cfg(test)]
mod tests;
pub(crate) mod workspaces;

/// Schema 87: the operator console.
pub(crate) struct OperatorConsole;
impl MigrationName for OperatorConsole {
    fn name(&self) -> &str {
        "m20261106_000087_operator_console"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for OperatorConsole {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("schema.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "The operator console requires a forward migration".into(),
        ))
    }
}

/// Where an operator made a change; the change log shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Via {
    Web,
    Cli,
}
impl Via {
    fn as_str(self) -> &'static str {
        match self {
            Via::Web => "web",
            Via::Cli => "cli",
        }
    }
}

/// The person making a change and where from. Only built after the platform
/// administrator check.
#[derive(Clone)]
pub(crate) struct Operator {
    pub user: User,
    pub via: Via,
}

/// Every change needs a reason a colleague can read later.
pub(crate) fn reason(text: &str) -> Result<String> {
    let text = text.trim();
    ensure!(
        text.chars().count() >= 4,
        "Say why you are making this change (at least 4 characters)"
    );
    one_line(text, 300, "reason")?;
    Ok(text.to_owned())
}

/// A short single line of text: no control characters, at most `max` characters.
pub(crate) fn one_line(text: &str, max: usize, what: &str) -> Result<()> {
    ensure!(
        text.chars().count() <= max && !text.chars().any(char::is_control),
        "Keep the {what} to one line of {max} characters"
    );
    Ok(())
}

/// Dollars as micro-dollars, for prices, caps and ceilings.
pub(crate) fn micros(usd: f64) -> Result<i64> {
    ensure!(
        usd.is_finite() && (0.0..=1_000_000.0).contains(&usd),
        "Use an amount from $0 to $1,000,000"
    );
    Ok((usd * 1_000_000.0).round() as i64)
}

/// Appends one entry to the change log, inside the caller's transaction.
pub(crate) async fn record(
    db: &impl ConnectionTrait,
    by: &Operator,
    area: &str,
    action: &str,
    target: &str,
    summary: &str,
    why: &str,
) -> Result<()> {
    execute(
        db,
        "INSERT INTO relay_operator_log(actor,via,area,action,target,summary,reason) VALUES($1,$2,$3,$4,$5,$6,$7)",
        vec![
            by.user.id.clone().into(),
            by.via.as_str().into(),
            area.into(),
            action.into(),
            target.chars().take(200).collect::<String>().into(),
            summary.chars().take(1000).collect::<String>().into(),
            why.into(),
        ],
    )
    .await?;
    Ok(())
}

const DAY_MS: i64 = 86_400_000;

/// The start of the UTC calendar month holding `ms`, in milliseconds.
pub(crate) fn month_start(ms: i64) -> i64 {
    let day = ms - ms.rem_euclid(DAY_MS);
    // Day of the month from days since 1970 (Howard Hinnant's civil-date algorithm).
    let doe = (day / DAY_MS + 719_468).rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let day_of_month = doy - (153 * ((5 * doy + 2) / 153) + 2) / 5; // 0-based
    day - day_of_month * DAY_MS
}

/// The start of the current UTC month and day, in milliseconds.
pub(crate) fn month_and_day_start() -> (i64, i64) {
    let now_ms = now() * 1000;
    (month_start(now_ms), now_ms - now_ms.rem_euclid(DAY_MS))
}

#[cfg(test)]
#[test]
fn months_start_on_the_first_in_utc() {
    // 2026-11-06T10:48:00Z → 2026-11-01; 2024-02-29 → 2024-02-01; 2027-01-01 → itself.
    assert_eq!(month_start(1_793_962_080_000), 1_793_491_200_000);
    assert_eq!(month_start(1_709_164_800_000), 1_706_745_600_000);
    assert_eq!(month_start(1_798_761_600_000), 1_798_761_600_000);
}

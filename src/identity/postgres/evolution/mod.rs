//! Agents that get better on their own (schema 143). Every answer gets an
//! outcome; the notes a request used are kept, so an accepted answer makes them
//! stronger and a correction puts the ones it contradicts on hold; new notes are
//! checked once by the classifier; requests are sorted into each agent's job
//! types; and every three days an agent reviews its routine work and proposes
//! skills, which one of its owners or an admin approves. The model calls are made
//! by the controller task in `crate::identity::evolution`; this module reads and
//! writes what they decide.
use super::*;

pub(crate) mod improving;
pub(crate) mod jobs;
pub(crate) mod notes;
pub(crate) mod outcomes;
pub(crate) mod proposals;
#[cfg(test)]
mod review_tests;
pub(crate) mod reviews;
#[cfg(test)]
mod tests;

/// A classifier answer acts at or above this confidence; below `ASK` it is dropped.
pub(crate) const ACT: f64 = 0.8;
pub(crate) const ASK: f64 = 0.5;
/// Requests a review reads, and how far back.
pub(crate) const LOOK_BACK_MS: i64 = 30 * 86_400_000;

pub(crate) async fn ready(db: &impl ConnectionTrait) -> Result<bool> {
    Ok(query(
        db,
        "SELECT to_regclass('relay_request_outcomes') IS NOT NULL AS ready",
        vec![],
    )
    .await?
    .context("Missing schema check")?
    .try_get("", "ready")?)
}

pub(crate) use migration::Evolution;
mod migration {
    use sea_orm_migration::prelude::*;
    pub(crate) struct Evolution;
    impl MigrationName for Evolution {
        fn name(&self) -> &str {
            "m20261124_000143_evolution"
        }
    }
    #[async_trait::async_trait]
    impl MigrationTrait for Evolution {
        async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
            m.get_connection()
                .execute_unprepared(include_str!("evolution.sql"))
                .await?;
            Ok(())
        }
        async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
            Err(DbErr::Custom(
                "Learning records require a forward migration".into(),
            ))
        }
    }
}

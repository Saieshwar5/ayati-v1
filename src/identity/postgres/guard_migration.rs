use sea_orm_migration::prelude::*;
/// Schema 64: the Guard's company rules and the decision record on actions.
pub(crate) struct Guard;
impl MigrationName for Guard {
    fn name(&self) -> &str {
        "m20261105_000064_guard"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for Guard {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("guard.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "The Guard requires a forward migration".into(),
        ))
    }
}

/// Schema 65: action meanings, safe defaults from them, rules for every action of a
/// meaning and time-of-day rules.
pub(crate) struct GuardMeanings;
impl MigrationName for GuardMeanings {
    fn name(&self) -> &str {
        "m20261105_000065_guard_meanings"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for GuardMeanings {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("guard_meanings.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Action meanings require a forward migration".into(),
        ))
    }
}

/// Schema 66: rules that use a value from an earlier read, and "read first" rules.
pub(crate) struct GuardFacts;
impl MigrationName for GuardFacts {
    fn name(&self) -> &str {
        "m20261105_000066_guard_facts"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for GuardFacts {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("guard_facts.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Guard facts require a forward migration".into(),
        ))
    }
}

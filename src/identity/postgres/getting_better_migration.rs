use sea_orm_migration::prelude::*;
/// Schema 63: goals, weekly reviews and rules from repeated decisions.
pub(crate) struct GettingBetter;
impl MigrationName for GettingBetter {
    fn name(&self) -> &str {
        "m20261102_000063_getting_better"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for GettingBetter {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("getting_better.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Getting better requires a forward migration".into(),
        ))
    }
}

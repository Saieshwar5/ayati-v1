use sea_orm_migration::prelude::*;
/// Schema 61: agents report what they need; Needs fixing groups it by cause.
pub(crate) struct Supervising;
impl MigrationName for Supervising {
    fn name(&self) -> &str {
        "m20261102_000061_supervising"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for Supervising {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("supervising.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Supervising requires a forward migration".into(),
        ))
    }
}

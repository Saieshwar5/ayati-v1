use sea_orm_migration::prelude::*;
/// Schema 62: steering running work; routines proposed in Chat.
pub(crate) struct Steering;
impl MigrationName for Steering {
    fn name(&self) -> &str {
        "m20261102_000062_steering"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for Steering {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("steering.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Steering requires a forward migration".into(),
        ))
    }
}

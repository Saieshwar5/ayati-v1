use sea_orm_migration::prelude::*;
/// Schema 59: panels update after their agent's work and on source events.
pub(crate) struct HomeLiving;
impl MigrationName for HomeLiving {
    fn name(&self) -> &str {
        "m20261101_000059_home_living"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for HomeLiving {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("home_living.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Living panels require a forward migration".into(),
        ))
    }
}

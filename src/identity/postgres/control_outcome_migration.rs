use sea_orm_migration::prelude::*;
/// Schema 107: settling unconfirmed writes and rechecking them at the provider.
pub(crate) struct UnconfirmedWrites;
impl MigrationName for UnconfirmedWrites {
    fn name(&self) -> &str {
        "m20261108_000107_unconfirmed_writes"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for UnconfirmedWrites {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("control_outcomes.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Settled outcomes require a forward migration".into(),
        ))
    }
}

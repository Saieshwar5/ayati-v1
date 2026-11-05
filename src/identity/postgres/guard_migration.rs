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

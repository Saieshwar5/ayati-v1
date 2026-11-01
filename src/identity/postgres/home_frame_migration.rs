use sea_orm_migration::prelude::*;
/// Schema 57: the agent's "what changed" note on each published panel update.
pub(super) struct HomeFrame;
impl MigrationName for HomeFrame {
    fn name(&self) -> &str {
        "m20261101_000057_home_frame"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for HomeFrame {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("home_frame.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "The panel frame requires a forward migration".into(),
        ))
    }
}

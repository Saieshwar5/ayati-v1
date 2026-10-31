use sea_orm_migration::prelude::*;
pub(super) struct Activity;
impl MigrationName for Activity {
    fn name(&self) -> &str {
        "m20261031_000048_activity"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for Activity {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("activity.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Activity requires a forward migration".into(),
        ))
    }
}

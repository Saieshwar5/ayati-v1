use sea_orm_migration::prelude::*;
pub(super) struct DataLifecycle;
impl MigrationName for DataLifecycle {
    fn name(&self) -> &str {
        "m20261029_000035_data_lifecycle"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for DataLifecycle {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("data_lifecycle.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Data lifecycle requires a forward migration".into(),
        ))
    }
}

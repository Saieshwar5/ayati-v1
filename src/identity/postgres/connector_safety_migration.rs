use sea_orm_migration::prelude::*;
pub(super) struct ConnectorSafety;
impl MigrationName for ConnectorSafety {
    fn name(&self) -> &str {
        "m20261031_000047_connector_safety"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for ConnectorSafety {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("connector_safety.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Connector safety requires a forward migration".into(),
        ))
    }
}

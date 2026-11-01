use sea_orm_migration::prelude::*;
pub(crate) struct ConnectorProviders;
impl MigrationName for ConnectorProviders {
    fn name(&self) -> &str {
        "m20261101_000053_connector_providers"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for ConnectorProviders {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("connector_providers.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Connector providers require a forward migration".into(),
        ))
    }
}

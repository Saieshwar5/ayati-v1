use sea_orm_migration::prelude::*;
pub(crate) struct Communications;
impl MigrationName for Communications {
    fn name(&self) -> &str {
        "m20261026_000032_native_communications"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for Communications {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("communications.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Communication receipts require a forward migration".into(),
        ))
    }
}

/// Schema 54: real transports, delivery reasons and read cursors.
pub(crate) struct CommunicationGateway;
impl MigrationName for CommunicationGateway {
    fn name(&self) -> &str {
        "m20261101_000054_communication_gateway"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for CommunicationGateway {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("gateway.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "The communication gateway requires a forward migration".into(),
        ))
    }
}

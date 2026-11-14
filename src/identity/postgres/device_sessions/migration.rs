use sea_orm_migration::prelude::*;

/// Schema 115: device sessions and one-time sign-in codes for the phone app.
pub(crate) struct DeviceSessions;
impl MigrationName for DeviceSessions {
    fn name(&self) -> &str {
        "m20261114_000115_device_sessions"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for DeviceSessions {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("device_sessions.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Device sessions require a forward migration".into(),
        ))
    }
}

/// Schema 116: push registrations for signed-in phones, and what was pushed.
pub(crate) struct PushDevices;
impl MigrationName for PushDevices {
    fn name(&self) -> &str {
        "m20261114_000116_push_devices"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for PushDevices {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("push.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Push devices require a forward migration".into(),
        ))
    }
}

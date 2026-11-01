use sea_orm_migration::prelude::*;
pub(crate) struct SharedRooms;
impl MigrationName for SharedRooms {
    fn name(&self) -> &str {
        "m20261101_000055_shared_rooms"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for SharedRooms {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("shared_rooms.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Shared rooms require a forward migration".into(),
        ))
    }
}

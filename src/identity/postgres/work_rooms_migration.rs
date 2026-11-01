use sea_orm_migration::prelude::*;
pub(crate) struct WorkRooms;
impl MigrationName for WorkRooms {
    fn name(&self) -> &str {
        "m20261101_000056_work_rooms"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for WorkRooms {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("work_rooms.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Work rooms require a forward migration".into(),
        ))
    }
}

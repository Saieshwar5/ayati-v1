use sea_orm_migration::prelude::*;
/// Schema 58: tasks, and each request's link to the task it works towards.
pub(crate) struct Tasks;
impl MigrationName for Tasks {
    fn name(&self) -> &str {
        "m20261101_000058_tasks"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for Tasks {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("tasks.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom("Tasks require a forward migration".into()))
    }
}

use sea_orm_migration::prelude::*;
pub(super) struct ExecutorTasks;
impl MigrationName for ExecutorTasks {
    fn name(&self) -> &str {
        "m20261029_000034_executor_tasks"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for ExecutorTasks {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("executor_tasks.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Executor task receipts require a forward migration".into(),
        ))
    }
}

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

/// Schema 144: tasks with a lifetime: calendar rules, end dates and run counts,
/// notes for the next run, and the workspace's working days and holidays.
pub(crate) struct TaskLifetimes;
impl MigrationName for TaskLifetimes {
    fn name(&self) -> &str {
        "m20261124_000144_task_lifetimes"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for TaskLifetimes {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("task_lifetimes.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Task lifetimes require a forward migration".into(),
        ))
    }
}

/// Schema 145: task rooms for channel tasks, instructions, checks in code, the
/// schedule a task came from, and minute rules (designs/agent-tasks/one-system.md).
pub(crate) struct TaskRooms;
impl MigrationName for TaskRooms {
    fn name(&self) -> &str {
        "m20261124_000145_task_rooms"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for TaskRooms {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("task_rooms.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Task rooms require a forward migration".into(),
        ))
    }
}

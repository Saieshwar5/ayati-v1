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

/// Schema 147: every schedule that is not a Home panel update becomes a task and
/// no longer runs as a schedule (designs/agent-tasks/one-system.md, decision 4).
pub(crate) struct SchedulesToTasks;
impl MigrationName for SchedulesToTasks {
    fn name(&self) -> &str {
        "m20261124_000147_schedules_to_tasks"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for SchedulesToTasks {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("schedules_to_tasks.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Schedules become tasks in a forward migration".into(),
        ))
    }
}

/// Schema 148: every event trigger gets a task; a delivered event starts a run of
/// it (designs/agent-tasks/one-system.md, decision 6).
pub(crate) struct TriggerTasks;
impl MigrationName for TriggerTasks {
    fn name(&self) -> &str {
        "m20261124_000148_trigger_tasks"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for TriggerTasks {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("trigger_tasks.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Event triggers become tasks in a forward migration".into(),
        ))
    }
}

/// Schema 149: an assistant's regular check becomes a task with a check in code
/// before the model (designs/agent-tasks/one-system.md, decision 7).
pub(crate) struct CheckTasks;
impl MigrationName for CheckTasks {
    fn name(&self) -> &str {
        "m20261124_000149_check_tasks"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for CheckTasks {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("check_tasks.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Regular checks become tasks in a forward migration".into(),
        ))
    }
}

/// Schema 150: the old schedule tables, columns and functions are dropped; tasks
/// are the only thing an agent does on its own (designs/agent-tasks/one-system.md).
pub(crate) struct DropSchedules;
impl MigrationName for DropSchedules {
    fn name(&self) -> &str {
        "m20261125_000150_drop_schedules"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for DropSchedules {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("drop_schedules.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Schedules were dropped in a forward migration".into(),
        ))
    }
}

/// Schema 151: a task run happens in the task's own conversation, as its person;
/// work rooms, trigger memory and posted results are dropped (the owner's one rule,
/// designs/agent-tasks/one-system.md, decision 1).
pub(crate) struct TasksInConversations;
impl MigrationName for TasksInConversations {
    fn name(&self) -> &str {
        "m20261125_000151_tasks_in_conversations"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for TasksInConversations {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("tasks_in_conversations.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Tasks moved into their conversations in a forward migration".into(),
        ))
    }
}

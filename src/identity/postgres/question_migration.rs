use sea_orm_migration::prelude::*;
pub(super) struct Questions;
impl MigrationName for Questions {
    fn name(&self) -> &str {
        "m20261026_000023_agent_questions"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for Questions {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(include_str!("questions.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Saved answers require a forward migration".into(),
        ))
    }
}

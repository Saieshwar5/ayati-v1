use sea_orm_migration::prelude::*;
pub(crate) struct Skills;
impl MigrationName for Skills {
    fn name(&self) -> &str {
        "m20261031_000050_skills"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for Skills {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("skills.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom("Skills require a forward migration".into()))
    }
}

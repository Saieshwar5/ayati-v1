use sea_orm_migration::prelude::*;

pub struct Migrator;
#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(Foundation)]
    }
}
struct Foundation;
impl MigrationName for Foundation {
    fn name(&self) -> &str {
        "m20261020_000001_workspace_foundation"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for Foundation {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(include_str!("schema.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Workspace data is durable; use a verified backup or a forward migration".into(),
        ))
    }
}
impl super::PgStore {
    pub async fn migrate(&self) -> anyhow::Result<()> {
        Migrator::up(&self.db, None).await?;
        Ok(())
    }
}

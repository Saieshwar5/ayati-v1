//! Schema 115 gave every event trigger a private work room and a Memory setting
//! (fresh or remember). Schema 151 removed both: a trigger runs in its
//! conversation. The migration stays in the chain.
pub(crate) use migration::AutomationMemory;

mod migration {
    use sea_orm_migration::prelude::*;
    pub(crate) struct AutomationMemory;
    impl MigrationName for AutomationMemory {
        fn name(&self) -> &str {
            "m20261114_000115_automation_memory"
        }
    }
    #[async_trait::async_trait]
    impl MigrationTrait for AutomationMemory {
        async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
            m.get_connection()
                .execute_unprepared(include_str!("automation_memory.sql"))
                .await?;
            Ok(())
        }
        async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
            Err(DbErr::Custom(
                "Automation memory requires a forward migration".into(),
            ))
        }
    }
}

//! Schema 104: what agents do in their computer's browser is recorded with their other
//! computer steps, for Activity (runtime/agent-computers.md).
use sea_orm::ConnectionTrait;
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};

pub(super) struct ComputerBrowserMigration;
impl MigrationName for ComputerBrowserMigration {
    fn name(&self) -> &str {
        "m20261108_000104_computer_browser"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for ComputerBrowserMigration {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(
                "ALTER TABLE relay_computer_steps DROP CONSTRAINT relay_computer_steps_kind_check;
                 ALTER TABLE relay_computer_steps ADD CONSTRAINT relay_computer_steps_kind_check
                   CHECK(kind IN ('command','list','read','write','browser'));",
            )
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Computer browser steps require a forward migration".into(),
        ))
    }
}

//! Schema 105 added workspace-wide limits on agent computers. They were removed on
//! 2026-11-19: an agent's computer follows only its own settings. The migration stays
//! so existing databases keep their history; nothing reads `relay_computer_policies`.
use sea_orm::ConnectionTrait;
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};

pub(super) struct ComputerPolicyMigration;
impl MigrationName for ComputerPolicyMigration {
    fn name(&self) -> &str {
        "m20261108_000105_computer_policies"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for ComputerPolicyMigration {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(
                "CREATE TABLE relay_computer_policies (
                  workspace TEXT PRIMARY KEY REFERENCES relay_workspaces(id),
                  policy JSONB NOT NULL CHECK(octet_length(policy::text) <= 20000),
                  updated_by TEXT NOT NULL,
                  updated_ms BIGINT NOT NULL);",
            )
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Computer policies require a forward migration".into(),
        ))
    }
}

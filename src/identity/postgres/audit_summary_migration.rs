use sea_orm_migration::prelude::*;
pub(super) struct AuditSummaries;
impl MigrationName for AuditSummaries {
    fn name(&self) -> &str {
        "m20261030_000044_audit_summaries"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for AuditSummaries {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("audit_summaries.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Audit summaries require a forward migration".into(),
        ))
    }
}

use sea_orm_migration::prelude::*;
/// Schema 146: a Home panel's fixed interval runs on the panel's own update queue,
/// not as a schedule (designs/agent-tasks/one-system.md, decision 5).
pub(crate) struct PanelIntervals;
impl MigrationName for PanelIntervals {
    fn name(&self) -> &str {
        "m20261124_000146_panel_intervals"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for PanelIntervals {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("home_intervals.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Panel intervals require a forward migration".into(),
        ))
    }
}

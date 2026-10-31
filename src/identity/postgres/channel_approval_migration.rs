use sea_orm_migration::prelude::*;
pub(super) struct ChannelApprovals;
impl MigrationName for ChannelApprovals {
    fn name(&self) -> &str {
        "m20261025_000021_channel_approvals"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for ChannelApprovals {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(include_str!("channel_approvals.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Approval delivery requires a forward migration".into(),
        ))
    }
}

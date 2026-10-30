use sea_orm_migration::prelude::*;
pub(super) struct InviteLinks;
impl MigrationName for InviteLinks {
    fn name(&self) -> &str {
        "m20261030_000042_invite_links"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for InviteLinks {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("invite_links.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Invite links require a forward migration".into(),
        ))
    }
}

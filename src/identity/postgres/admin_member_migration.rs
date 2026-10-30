use sea_orm_migration::prelude::*;
pub(super) struct AdminMemberRoles;
impl MigrationName for AdminMemberRoles {
    fn name(&self) -> &str {
        "m20261030_000043_admin_member_roles"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for AdminMemberRoles {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("admin_member_roles.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Workspace role names require a forward migration".into(),
        ))
    }
}

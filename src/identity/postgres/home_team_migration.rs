use sea_orm_migration::prelude::*;
/// Schema 60: team panels, shown to a channel or the workspace.
pub(crate) struct HomeTeam;
impl MigrationName for HomeTeam {
    fn name(&self) -> &str {
        "m20261101_000060_home_team"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for HomeTeam {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("home_team.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Team panels require a forward migration".into(),
        ))
    }
}

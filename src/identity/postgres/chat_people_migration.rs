use sea_orm_migration::prelude::*;
pub(super) struct ChatPeople;
impl MigrationName for ChatPeople {
    fn name(&self) -> &str {
        "m20261030_000036_chat_people"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for ChatPeople {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("chat_people.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Chat people require a forward migration".into(),
        ))
    }
}

//! Schema 155 gave each agent a Library folder with a subfolder per conversation. Schema
//! 159 removed them: a conversation's files belong to the conversation
//! (`conversation_files`). Only the migration remains, for databases that ran it.
use sea_orm::ConnectionTrait;
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};

pub(crate) struct LibraryAgentFolders;
impl MigrationName for LibraryAgentFolders {
    fn name(&self) -> &str {
        "m20261125_000155_library_agent_folders"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for LibraryAgentFolders {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("agent_folders.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Agents' Library folders require a forward migration".into(),
        ))
    }
}

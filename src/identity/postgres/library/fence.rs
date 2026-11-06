//! Schema 71: Library edits and sharing changes stop raising the workspace-wide
//! context epoch; per-item receipts fence exactly the work that used an item.
use sea_orm::ConnectionTrait;
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};

pub(crate) struct LibraryFence;
impl MigrationName for LibraryFence {
    fn name(&self) -> &str {
        "m20261105_000071_library_fence"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for LibraryFence {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("fence.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "The Library fence requires a forward migration".into(),
        ))
    }
}

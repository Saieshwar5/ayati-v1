//! Descriptions (schema 158): one line saying what an item is, written by whoever saves
//! it, in the words people would look for. It is indexed with the item's text, so word
//! search finds a file by what it is about, with no model reading every document.
use super::*;
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};
use serde::Deserialize;

pub(crate) struct LibraryDescriptions;
impl MigrationName for LibraryDescriptions {
    fn name(&self) -> &str {
        "m20261125_000158_library_descriptions"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for LibraryDescriptions {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("descriptions.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Library descriptions require a forward migration".into(),
        ))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Description {
    pub text: String,
}

/// The description as kept: one line of up to 300 characters.
pub(super) fn tidy(text: &str) -> Result<String> {
    let text: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    ensure!(
        text.chars().count() <= 300 && !text.chars().any(char::is_control),
        "Describe it in one line of up to 300 characters"
    );
    Ok(text)
}

/// Sets an item's description and indexes it with the item's text. The caller checked
/// who may.
pub(super) async fn set(db: &impl ConnectionTrait, w: &str, id: &str, text: &str) -> Result<()> {
    execute(
        db,
        "UPDATE relay_library_items SET description=$3 WHERE workspace=$1 AND id=$2",
        vec![w.into(), id.into(), text.into()],
    )
    .await?;
    passages::index_item(db, w, id).await
}

impl PgStore {
    /// People who can edit an item say what it is.
    pub(crate) async fn library_describe(
        &self,
        user: &User,
        w: &str,
        id: &str,
        input: &Description,
    ) -> Result<Value> {
        let text = tidy(&input.text)?;
        let tx = self.workspace_tx(w).await?;
        let current = item(&tx, user, w, id, false).await?;
        ensure!(
            current["editable"] == true,
            "Only editors describe this item"
        );
        set(&tx, w, id, &text).await?;
        tx.commit().await?;
        Ok(json!({"description":text}))
    }
}

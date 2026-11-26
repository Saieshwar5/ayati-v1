//! Folders inside folders and admins' access (schema 153). An item follows the nearest
//! folder above it that has its own sharing; a subfolder follows its parent until it is
//! shared itself. Admins can open and manage everything, and opening what was not
//! shared with them is written to the Audit log.
use super::*;
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};

pub(crate) struct LibraryNested;
impl MigrationName for LibraryNested {
    fn name(&self) -> &str {
        "m20261125_000153_library_nested"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for LibraryNested {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("nested.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Nested Library folders require a forward migration".into(),
        ))
    }
}

/// Folders above an item, at most: a folder may sit four deep.
pub(super) const DEPTH: i64 = 5;

/// Refuses putting `id` (with whatever is inside it) into `folder` when that would make
/// a loop or nest too deep.
pub(super) async fn fits(
    db: &impl ConnectionTrait,
    w: &str,
    id: Option<&str>,
    folder: &str,
) -> Result<()> {
    let row = query(db, "SELECT relay_library_path($1,$2) AS path, COALESCE((WITH RECURSIVE down(id, depth) AS (SELECT id, 1 FROM relay_library_items WHERE workspace=$1 AND folder=$3 UNION ALL SELECT c.id, down.depth+1 FROM down JOIN relay_library_items c ON c.workspace=$1 AND c.folder=down.id WHERE down.depth<9) SELECT max(depth) FROM down),0)::bigint AS below", vec![w.into(), folder.into(), id.map(str::to_owned).into()]).await?.context("Folder unavailable")?;
    let path: Vec<String> = row.try_get("", "path")?;
    ensure!(
        id.is_none_or(|id| !path.iter().any(|p| p == id)),
        "A folder can't go inside itself"
    );
    ensure!(
        path.len() as i64 + 1 + row.try_get::<i64>("", "below")? <= DEPTH,
        "Folders can be nested up to five levels"
    );
    Ok(())
}

/// Sharing changed for everything inside this folder: their source receipts must see it.
pub(super) async fn touch_inside(db: &impl ConnectionTrait, w: &str, id: &str) -> Result<()> {
    execute(db, "UPDATE relay_library_items SET access_revision=access_revision+1 WHERE workspace=$1 AND id IN (SELECT relay_library_inside($1,$2))", vec![w.into(), id.into()]).await?;
    Ok(())
}

/// Writes to the Audit log that an admin opened something not shared with them, once an
/// hour per admin and item so a page left open doesn't fill the log.
pub(super) async fn admin_opened(db: &PgStore, user: &User, w: &str, item: &Value) -> Result<()> {
    if item["granted"] != false {
        return Ok(());
    }
    let id = item["id"].as_str().unwrap_or_default();
    let tx = db.workspace_tx(w).await?;
    if query(&tx, "SELECT 1 AS seen FROM relay_audit WHERE actor=$1 AND workspace=$2 AND action='library_admin_opened' AND target=$3 AND at>$4 LIMIT 1", vec![user.id.clone().into(), w.into(), id.into(), (now() - 3600).into()]).await?.is_none() {
        audit_with_summary(&tx, &user.id, Some(w), "library_admin_opened", id, &format!("Opened \"{}\" as an admin; it was not shared with them", item["title"].as_str().unwrap_or("Library item"))).await?;
    }
    tx.commit().await?;
    Ok(())
}

impl PgStore {
    /// Every folder the person was given, with the folder it is in, for the folder tree
    /// and for choosing where to move something.
    pub(crate) async fn library_folders(&self, user: &User, w: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        role(&tx, w, &user.id).await?;
        let folders = json_rows(&tx, "SELECT jsonb_build_object('id',i.id,'title',i.title,'folder',i.folder,'editable',relay_library_granted(i.workspace,i.id,$2)='edit') AS data FROM relay_library_items i WHERE i.workspace=$1 AND i.kind='folder' AND NOT i.trashed AND relay_library_granted(i.workspace,i.id,$2) IS NOT NULL ORDER BY lower(i.title),i.id LIMIT 500", vec![w.into(), user.id.clone().into()]).await?;
        tx.commit().await?;
        Ok(json!({"folders":folders}))
    }
}

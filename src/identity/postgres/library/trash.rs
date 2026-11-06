//! Emptying Trash (LB7): an item's creator deletes it for good from Trash, one at a
//! time or all at once, except what is on legal hold. Stored bytes left by uploads that
//! were cancelled or never finished are removed too, once nothing uses them. Like the
//! keeping sweep, the rows go first and the caller removes the stored files.
use super::keeping::{held, purge};
use super::*;

/// Items deleted at once when emptying Trash, at most.
const EMPTY: i64 = 200;

impl PgStore {
    /// Deletes one trashed item for good; returns the stored file to remove.
    pub(crate) async fn library_delete_forever(
        &self,
        user: &User,
        w: &str,
        id: &str,
    ) -> Result<Vec<String>> {
        let tx = self.workspace_tx(w).await?;
        let current = item(&tx, user, w, id, true).await?;
        ensure!(
            current["trashed"] == true && current["purged"].is_null(),
            "Move it to Trash first"
        );
        ensure!(
            current["manageable"] == true,
            "Only its creator deletes it for good"
        );
        ensure!(
            !held(&tx, w, id).await?,
            "This is on legal hold; an admin must lift the hold first"
        );
        let mut files = vec![];
        if let Some(gone) = purge(&tx, w, id, now()).await? {
            files.extend(gone.file);
            audit_with_summary(
                &tx,
                &user.id,
                Some(w),
                "library_deleted_forever",
                id,
                &format!("Deleted \"{}\" for good from Trash", gone.title),
            )
            .await?;
        }
        tx.commit().await?;
        Ok(files)
    }

    /// Deletes everything the person may in their Trash for good, except held items.
    pub(crate) async fn library_empty_trash(
        &self,
        user: &User,
        w: &str,
    ) -> Result<(usize, Vec<String>)> {
        let tx = self.workspace_tx(w).await?;
        role(&tx, w, &user.id).await?;
        let ids: Vec<String> = rows(&tx, &format!("SELECT i.id FROM relay_library_items i WHERE i.workspace=$1 AND i.trashed AND i.purged IS NULL AND i.steward=$2 AND NOT EXISTS(SELECT 1 FROM relay_library_holds h WHERE h.workspace=i.workspace AND h.item IN (i.id, i.folder)) ORDER BY i.updated LIMIT {EMPTY}"), vec![w.into(), user.id.clone().into()]).await?.into_iter().map(|r| r.try_get("", "id")).collect::<std::result::Result<_, _>>()?;
        let mut files = vec![];
        for id in &ids {
            if let Some(gone) = purge(&tx, w, id, now()).await? {
                files.extend(gone.file);
            }
        }
        if !ids.is_empty() {
            audit_with_summary(
                &tx,
                &user.id,
                Some(w),
                "library_trash_emptied",
                w,
                &format!(
                    "Emptied Trash: {} item{} deleted for good",
                    ids.len(),
                    if ids.len() == 1 { "" } else { "s" }
                ),
            )
            .await?;
        }
        tx.commit().await?;
        Ok((ids.len(), files))
    }

    /// Stored bytes of uploads cancelled or never finished, that no item uses; their
    /// sessions are closed so each file is returned once.
    pub(crate) async fn library_orphaned_uploads(&self, at: i64) -> Result<Vec<String>> {
        let gone = rows(&self.db, "UPDATE relay_library_uploads u SET state='cleaned' WHERE (u.workspace,u.id) IN (SELECT x.workspace,x.id FROM relay_library_uploads x WHERE (x.state='cancelled' OR (x.state='pending' AND x.expires<$1)) AND NOT EXISTS(SELECT 1 FROM relay_library_items i WHERE i.workspace=x.workspace AND i.object_key=x.object_key) LIMIT 200) RETURNING u.object_key", vec![at.into()]).await?;
        gone.into_iter()
            .map(|r| Ok(r.try_get("", "object_key")?))
            .collect()
    }
}

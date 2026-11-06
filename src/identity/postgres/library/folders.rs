//! Library folders (LB1): one level of folders whose sharing their pages and files follow.
use super::*;
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};

/// Schema 69: folders, agent access levels with "use for", and agent read receipts.
pub(crate) struct LibraryFolders;
impl MigrationName for LibraryFolders {
    fn name(&self) -> &str {
        "m20261105_000069_library_folders"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for LibraryFolders {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("folders.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Library folders require a forward migration".into(),
        ))
    }
}

/// A folder this person may file items into: it exists, is not in Trash and they can edit it.
pub(super) async fn open_folder(
    db: &impl ConnectionTrait,
    user: &User,
    w: &str,
    id: &str,
) -> Result<Value> {
    let folder = item(db, user, w, id, false).await?;
    ensure!(folder["kind"] == "folder", "Choose a folder");
    ensure!(
        folder["editable"] == true,
        "You need edit access to the folder to add items to it"
    );
    Ok(folder)
}

impl PgStore {
    pub async fn library_folder_create(
        &self,
        user: &User,
        w: &str,
        input: &CreateFolder,
    ) -> Result<Value> {
        uuid(&input.id)?;
        title(&input.title)?;
        let payload = json!({"folder":input});
        let tx = self.workspace_tx(w).await?;
        role(&tx, w, &user.id).await?;
        if let Some(saved) = retry(&tx, user, w, &input.request, &payload).await? {
            tx.commit().await?;
            return Ok(saved);
        }
        let count: i64 = query(
            &tx,
            "SELECT count(*) AS n FROM relay_library_items WHERE workspace=$1",
            vec![w.into()],
        )
        .await?
        .context("Library count unavailable")?
        .try_get("", "n")?;
        ensure!(
            count < 1000,
            "This workspace's initial Library limit is 1,000 items"
        );
        execute(&tx,"INSERT INTO relay_library_items(workspace,id,kind,title,created_by,steward,created,updated) VALUES($1,$2,'folder',$3,$4,$4,$5,$5)",vec![w.into(),input.id.clone().into(),input.title.trim().into(),user.id.clone().into(),now().into()]).await?;
        let result = json!({"id":input.id});
        remember(&tx, user, w, &input.request, payload, &result).await?;
        audit_with_summary(
            &tx,
            &user.id,
            Some(w),
            "library_folder_created",
            &input.id,
            &format!("Created the Library folder \"{}\"", input.title.trim()),
        )
        .await?;
        tx.commit().await?;
        Ok(result)
    }

    /// Files an item into a folder, or takes it out. Its own sharing is cleared both
    /// ways: in a folder it follows the folder; out of one it starts private again.
    pub async fn library_move(
        &self,
        user: &User,
        w: &str,
        id: &str,
        input: &MoveItem,
    ) -> Result<Value> {
        let tx = self.workspace_tx(w).await?;
        let current = item(&tx, user, w, id, false).await?;
        let payload = json!({"move":id,"input":input});
        if let Some(result) = retry(&tx, user, w, &input.request, &payload).await? {
            tx.commit().await?;
            return Ok(result);
        }
        ensure!(
            current["kind"] != "folder",
            "Folders can't go inside folders"
        );
        ensure!(
            current["manageable"] == true,
            "Only the creator can move this item"
        );
        if current["access_revision"].as_i64() != Some(input.revision) {
            return Err(Conflict.into());
        }
        let target = match &input.folder {
            Some(folder) => Some(open_folder(&tx, user, w, folder).await?),
            None => None,
        };
        for table in ["relay_library_people", "relay_library_agents"] {
            execute(
                &tx,
                &format!("DELETE FROM {table} WHERE workspace=$1 AND item=$2"),
                vec![w.into(), id.into()],
            )
            .await?;
        }
        execute(&tx,"UPDATE relay_library_items SET folder=$3,visibility='private',access_revision=access_revision+1 WHERE workspace=$1 AND id=$2",vec![w.into(),id.into(),input.folder.clone().into()]).await?;
        let name = current["title"].as_str().unwrap_or("Library item");
        audit_with_summary(
            &tx,
            &user.id,
            Some(w),
            "library_moved",
            id,
            &match &target {
                Some(folder) => format!(
                    "Moved \"{name}\" to the folder \"{}\"",
                    folder["title"].as_str().unwrap_or("Folder")
                ),
                None => format!("Took \"{name}\" out of its folder; it is private again"),
            },
        )
        .await?;
        super::super::task_revocation::inaccessible(&tx, w).await?;
        let result = json!({"ok":true});
        remember(&tx, user, w, &input.request, payload, &result).await?;
        tx.commit().await?;
        Ok(result)
    }
}

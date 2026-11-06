//! Keeping rules and legal holds (LB7). An admin sets how long a folder keeps what is
//! added to it; past that, the sweep deletes the item for good — its text, versions,
//! passages, meaning and stored file — leaving a trashed tombstone so answers that
//! cited it stay hidden and it can't be restored. A legal hold on an item, or on its
//! folder, stops trashing and deleting and keeps every version until an admin lifts it.
use super::*;
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};
use serde::Deserialize;

/// Schema 82: keeping rules, legal holds and purged items.
pub(crate) struct LibraryKeeping;
impl MigrationName for LibraryKeeping {
    fn name(&self) -> &str {
        "m20261105_000082_library_keeping"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for LibraryKeeping {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("keeping.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Library keeping requires a forward migration".into(),
        ))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Keeping {
    /// Days a folder keeps what is added to it, or none to keep it.
    pub keep_days: Option<i32>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Hold {
    /// Why it is held; none lifts the hold.
    pub reason: Option<String>,
}

/// Items deleted for good in one sweep, at most.
const SWEEP: i64 = 200;
/// Everything kept per item besides the item row itself, emptied when it is deleted.
const ITEM_TABLES: &[&str] = &[
    "relay_library_versions",
    "relay_library_passages",
    "relay_library_embeddings",
    "relay_library_summaries",
    "relay_library_suggestions",
    "relay_library_people",
    "relay_library_agents",
    "relay_library_stars",
    "relay_library_always",
    "relay_library_reviews",
    "relay_library_indexed",
];

async fn present(db: &impl ConnectionTrait) -> Result<bool> {
    Ok(query(
        db,
        "SELECT to_regclass('relay_library_holds') IS NOT NULL AS present",
        vec![],
    )
    .await?
    .context("Schema check")?
    .try_get("", "present")?)
}

/// Whether the item, or the folder it is in, is on legal hold.
pub(super) async fn held(db: &impl ConnectionTrait, w: &str, id: &str) -> Result<bool> {
    if !present(db).await? {
        return Ok(false);
    }
    Ok(query(db, "SELECT 1 AS found FROM relay_library_items i JOIN relay_library_holds h ON h.workspace=i.workspace AND h.item IN (i.id, i.folder) WHERE i.workspace=$1 AND i.id=$2", vec![w.into(), id.into()]).await?.is_some())
}
/// Whether the item was deleted for good.
pub(super) async fn purged(db: &impl ConnectionTrait, w: &str, id: &str) -> Result<bool> {
    if !present(db).await? {
        return Ok(false);
    }
    Ok(query(db, "SELECT 1 AS found FROM relay_library_items WHERE workspace=$1 AND id=$2 AND purged IS NOT NULL", vec![w.into(), id.into()]).await?.is_some())
}

/// What [`purge`] removed: the item's title, whether it was in a folder, and the stored
/// file to delete once the transaction commits.
pub(super) struct Purged {
    pub title: String,
    pub filed: bool,
    pub file: Option<String>,
}

/// Deletes an item for good inside the caller's transaction: everything kept for it
/// goes, and a trashed tombstone stays so answers that cited it stay hidden. The caller
/// checks who may and that it isn't held; none when it was already gone.
pub(super) async fn purge(
    tx: &DatabaseTransaction,
    w: &str,
    id: &str,
    at: i64,
) -> Result<Option<Purged>> {
    let Some(gone) = query(tx, "SELECT title,object_key,folder FROM relay_library_items WHERE workspace=$1 AND id=$2 AND purged IS NULL FOR UPDATE", vec![w.into(), id.into()]).await? else {
        return Ok(None);
    };
    for table in ITEM_TABLES {
        // Meaning search tables exist only where pgvector is installed.
        let exists: bool = query(
            tx,
            "SELECT to_regclass($1) IS NOT NULL AS present",
            vec![(*table).into()],
        )
        .await?
        .context("Schema check")?
        .try_get("", "present")?;
        if exists {
            execute(
                tx,
                &format!("DELETE FROM {table} WHERE workspace=$1 AND item=$2"),
                vec![w.into(), id.into()],
            )
            .await?;
        }
    }
    execute(tx, "UPDATE relay_library_items SET title='Deleted for good',document=NULL,text='',object_key=NULL,size=0,checksum='',mime='',folder=NULL,visibility='private',trashed=true,purged=$3,access_revision=access_revision+1,revision=revision+1,updated=$3 WHERE workspace=$1 AND id=$2", vec![w.into(), id.into(), at.into()]).await?;
    Ok(Some(Purged {
        title: gone.try_get("", "title")?,
        filed: gone.try_get::<Option<String>>("", "folder")?.is_some(),
        file: gone.try_get("", "object_key")?,
    }))
}

impl PgStore {
    /// The item's legal hold and, for a folder, its keeping rule.
    pub(crate) async fn library_keeping(&self, user: &User, w: &str, id: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        let current = item(&tx, user, w, id, false).await?;
        let admin = role(&tx, w, &user.id).await? == "Admin";
        let keep = query(
            &tx,
            "SELECT keep_days FROM relay_library_keeping WHERE workspace=$1 AND folder=$2",
            vec![w.into(), id.into()],
        )
        .await?
        .map(|r| r.try_get::<i32>("", "keep_days"))
        .transpose()?;
        let rule = match current["folder"].as_str() {
            Some(folder) => query(
                &tx,
                "SELECT keep_days FROM relay_library_keeping WHERE workspace=$1 AND folder=$2",
                vec![w.into(), folder.into()],
            )
            .await?
            .map(|r| r.try_get::<i32>("", "keep_days"))
            .transpose()?,
            None => None,
        };
        let hold = json_rows(&tx, "SELECT jsonb_build_object('item',h.item,'reason',h.reason,'at',h.held_at,'by',(SELECT COALESCE(u.display_name,u.email) FROM relay_users u WHERE u.id=h.held_by),'on_folder',h.item<>$2) AS data FROM relay_library_items i JOIN relay_library_holds h ON h.workspace=i.workspace AND h.item IN (i.id, i.folder) WHERE i.workspace=$1 AND i.id=$2 ORDER BY h.item=$2 DESC LIMIT 1", vec![w.into(), id.into()]).await?.into_iter().next();
        tx.commit().await?;
        Ok(
            json!({"keep_days":keep,"folder_keep_days":rule,"deletes_at":rule.map(|d| current["created"].as_i64().unwrap_or(0) + i64::from(d) * 86_400),"hold":hold,"can_change":admin}),
        )
    }

    /// Admins set how long a folder keeps what is added to it.
    pub(crate) async fn library_keeping_set(
        &self,
        user: &User,
        w: &str,
        id: &str,
        input: &Keeping,
    ) -> Result<Value> {
        ensure!(
            input.keep_days.is_none_or(|d| (1..=3650).contains(&d)),
            "Keep items 1 to 3,650 days"
        );
        let tx = self.workspace_tx(w).await?;
        ensure!(
            role(&tx, w, &user.id).await? == "Admin",
            "Only admins set keeping rules"
        );
        let current = item(&tx, user, w, id, false).await?;
        ensure!(
            current["kind"] == "folder",
            "Keeping rules are set on folders"
        );
        let title = current["title"].as_str().unwrap_or("").to_owned();
        let said = match input.keep_days {
            Some(days) => {
                execute(&tx, "INSERT INTO relay_library_keeping(workspace,folder,keep_days,set_by,set_at) VALUES($1,$2,$3,$4,$5) ON CONFLICT(workspace,folder) DO UPDATE SET keep_days=EXCLUDED.keep_days,set_by=EXCLUDED.set_by,set_at=EXCLUDED.set_at", vec![w.into(), id.into(), days.into(), user.id.clone().into(), now().into()]).await?;
                format!("Items in \"{title}\" are deleted {days} days after they are added")
            }
            None => {
                execute(
                    &tx,
                    "DELETE FROM relay_library_keeping WHERE workspace=$1 AND folder=$2",
                    vec![w.into(), id.into()],
                )
                .await?;
                format!("Items in \"{title}\" are kept")
            }
        };
        audit_with_summary(&tx, &user.id, Some(w), "library_keeping", id, &said).await?;
        tx.commit().await?;
        Ok(json!({"ok":true}))
    }

    /// Admins put an item or folder on legal hold, or lift it.
    pub(crate) async fn library_hold_set(
        &self,
        user: &User,
        w: &str,
        id: &str,
        input: &Hold,
    ) -> Result<Value> {
        let reason = input.reason.as_deref().map(str::trim);
        ensure!(
            reason.is_none_or(|r| !r.is_empty() && r.chars().count() <= 300),
            "Say why, in up to 300 characters"
        );
        let tx = self.workspace_tx(w).await?;
        ensure!(
            role(&tx, w, &user.id).await? == "Admin",
            "Only admins put items on legal hold"
        );
        let current = item(&tx, user, w, id, true).await?;
        ensure!(
            current["purged"].is_null(),
            "This item was deleted for good"
        );
        let title = current["title"].as_str().unwrap_or("").to_owned();
        let said = match reason {
            Some(reason) => {
                execute(&tx, "INSERT INTO relay_library_holds(workspace,item,reason,held_by,held_at) VALUES($1,$2,$3,$4,$5) ON CONFLICT(workspace,item) DO UPDATE SET reason=EXCLUDED.reason", vec![w.into(), id.into(), reason.into(), user.id.clone().into(), now().into()]).await?;
                format!("Put \"{title}\" on legal hold: {reason}")
            }
            None => {
                execute(
                    &tx,
                    "DELETE FROM relay_library_holds WHERE workspace=$1 AND item=$2",
                    vec![w.into(), id.into()],
                )
                .await?;
                format!("Lifted the legal hold on \"{title}\"")
            }
        };
        audit_with_summary(&tx, &user.id, Some(w), "library_hold", id, &said).await?;
        tx.commit().await?;
        Ok(json!({"ok":true}))
    }

    /// Deletes for good what keeping rules say is past its time, skipping anything on
    /// hold. Returns the stored files to remove; the rows are already gone.
    pub(crate) async fn library_keeping_sweep(&self, at: i64) -> Result<Vec<String>> {
        if !present(&self.db).await? {
            return Ok(vec![]);
        }
        let due = rows(&self.db, &format!("SELECT i.workspace,i.id FROM relay_library_items i JOIN relay_library_keeping k ON k.workspace=i.workspace AND k.folder=i.folder WHERE i.purged IS NULL AND i.kind<>'folder' AND i.created + k.keep_days::bigint*86400 < $1 AND NOT EXISTS(SELECT 1 FROM relay_library_holds h WHERE h.workspace=i.workspace AND h.item IN (i.id, i.folder)) ORDER BY i.created LIMIT {SWEEP}"), vec![at.into()]).await?;
        let mut files = vec![];
        for row in due {
            let (w, id): (String, String) = (row.try_get("", "workspace")?, row.try_get("", "id")?);
            let tx = self.workspace_tx(&w).await?;
            if held(&tx, &w, &id).await? {
                tx.commit().await?;
                continue;
            }
            if let Some(gone) = purge(&tx, &w, &id, at).await? {
                files.extend(gone.file);
                let said = format!(
                    "Deleted \"{}\" for good under the keeping rule{}",
                    gone.title,
                    if gone.filed { " of its folder" } else { "" }
                );
                audit_with_summary(&tx, "system", Some(&w), "library_purged", &id, &said).await?;
            }
            tx.commit().await?;
        }
        Ok(files)
    }
}

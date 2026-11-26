//! File versions (schema 152). A file keeps versions like a page: replacing it adds one,
//! restoring one makes the next, and History lists them. Each version names the stored
//! object it was. The latest 20 are kept; the objects of versions dropped are removed
//! from storage by the keeping sweep once nothing names them.
use super::*;
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};

pub(crate) struct LibraryFileVersions;
impl MigrationName for LibraryFileVersions {
    fn name(&self) -> &str {
        "m20261125_000152_library_file_versions"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for LibraryFileVersions {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("file_versions.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Library file versions require a forward migration".into(),
        ))
    }
}

/// Versions kept per file; pages and tables keep 50.
pub(super) const FILE_VERSIONS: i64 = 20;
const FILE_COLUMNS: &str = "object_key,mime,size,checksum,readiness";

/// Whether this database keeps file versions (upgrade tests run older schemas).
pub(super) async fn present(db: &impl ConnectionTrait) -> Result<bool> {
    Ok(query(
        db,
        "SELECT to_regclass('relay_library_discarded') IS NOT NULL AS present",
        vec![],
    )
    .await?
    .context("Schema check")?
    .try_get("", "present")?)
}

/// Records the item as it is now as a version by `author`, for an agent's task if any.
pub(in crate::identity::postgres) async fn snapshot(
    db: &impl ConnectionTrait,
    w: &str,
    id: &str,
    author: &str,
    agent: Option<(&str, &str)>,
) -> Result<()> {
    let files = if present(db).await? {
        format!(",{FILE_COLUMNS}")
    } else {
        String::new()
    };
    let kinds = if files.is_empty() {
        "'page','table'"
    } else {
        "'page','table','file'"
    };
    execute(db, &format!("INSERT INTO relay_library_versions(workspace,item,revision,author,title,document,text,created,agent,job{files}) SELECT workspace,id,revision,$3,title,document,text,$4,$5,$6{files} FROM relay_library_items WHERE workspace=$1 AND id=$2 AND kind IN ({kinds})"), vec![w.into(), id.into(), author.into(), now().into(), agent.map(|a| a.0.to_owned()).into(), agent.map(|a| a.1.to_owned()).into()]).await?;
    Ok(())
}

/// Bytes of stored files a workspace keeps: every version's object once, and uploads
/// still under way.
pub(in crate::identity::postgres) async fn storage_used(
    db: &impl ConnectionTrait,
    w: &str,
) -> Result<i64> {
    let stored = if present(db).await? {
        "SELECT coalesce(sum(size),0)::bigint FROM (SELECT DISTINCT object_key,size FROM relay_library_versions WHERE workspace=$1 AND object_key IS NOT NULL) v"
    } else {
        "SELECT coalesce(sum(size),0)::bigint FROM relay_library_items WHERE workspace=$1"
    };
    Ok(query(db, &format!("SELECT ({stored})+(SELECT coalesce(sum(size),0)::bigint FROM relay_library_uploads WHERE workspace=$1 AND state='pending' AND expires>$2) AS used"), vec![w.into(), now().into()]).await?.context("Storage use")?.try_get("", "used")?)
}

/// Every stored object an item's versions name, to remove once the item is deleted.
pub(super) async fn objects(db: &impl ConnectionTrait, w: &str, id: &str) -> Result<Vec<String>> {
    if !present(db).await? {
        return Ok(vec![]);
    }
    rows(db, "SELECT DISTINCT object_key FROM relay_library_versions WHERE workspace=$1 AND item=$2 AND object_key IS NOT NULL", vec![w.into(), id.into()]).await?.into_iter().map(|r| Ok(r.try_get("", "object_key")?)).collect()
}

/// Makes the file what `version` holds, as the next revision. The caller checked who may.
pub(super) async fn restore(
    db: &impl ConnectionTrait,
    w: &str,
    id: &str,
    version: Option<i64>,
) -> Result<()> {
    let changed = execute(db, "UPDATE relay_library_items i SET title=v.title,text=v.text,object_key=v.object_key,mime=v.mime,size=v.size,checksum=v.checksum,readiness=v.readiness,revision=i.revision+1,updated=$4 FROM relay_library_versions v WHERE i.workspace=$1 AND i.id=$2 AND v.workspace=i.workspace AND v.item=i.id AND v.revision=$3 AND v.object_key IS NOT NULL", vec![w.into(), id.into(), version.into(), now().into()]).await?;
    ensure!(changed == 1, "Version unavailable");
    Ok(())
}

impl PgStore {
    /// A file as one of its versions held it, for people who can edit it.
    pub async fn library_file_version(
        &self,
        user: &User,
        w: &str,
        id: &str,
        version: i64,
    ) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        let current = item(&tx, user, w, id, false).await?;
        ensure!(current["kind"] == "file", "This item is a page");
        ensure!(
            current["editable"] == true || current["revision"].as_i64() == Some(version),
            "History requires edit access"
        );
        let value = query(&tx, "SELECT jsonb_build_object('title',title,'object_key',object_key,'mime',mime,'size',size,'checksum',checksum) AS data FROM relay_library_versions WHERE workspace=$1 AND item=$2 AND revision=$3 AND object_key IS NOT NULL", vec![w.into(), id.into(), version.into()]).await?.context("Version unavailable")?.try_get("", "data")?;
        tx.commit().await?;
        super::nested::admin_opened(self, user, w, &current).await?;
        Ok(value)
    }

    /// Stored objects of dropped versions that nothing names any more; each is
    /// returned once for the caller to remove from storage.
    pub(crate) async fn library_discarded_objects(&self) -> Result<Vec<String>> {
        if !present(&self.db).await? {
            return Ok(vec![]);
        }
        let gone = rows(&self.db, "DELETE FROM relay_library_discarded d WHERE (d.workspace,d.object_key) IN (SELECT workspace,object_key FROM relay_library_discarded ORDER BY at LIMIT 200) RETURNING d.object_key, (EXISTS(SELECT 1 FROM relay_library_versions v WHERE v.workspace=d.workspace AND v.object_key=d.object_key) OR EXISTS(SELECT 1 FROM relay_library_items i WHERE i.workspace=d.workspace AND i.object_key=d.object_key)) AS used", vec![]).await?;
        let mut files = vec![];
        for row in gone {
            if !row.try_get::<bool>("", "used")? {
                files.push(row.try_get("", "object_key")?);
            }
        }
        Ok(files)
    }
}

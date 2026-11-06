//! A built app's code (Apps BA1): the builder changes draft files and publishes them as
//! numbered versions once the checks pass; its builder or an admin makes an earlier
//! version live or takes it offline; people allowed run the published version.
use super::*;
use crate::identity::built_apps::files as built_apps;
use serde::Deserialize;
use serde_json::json;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AppFile {
    pub path: String,
    /// The new content, or none to delete the file.
    pub content: Option<String>,
    /// The draft revision this change was made from.
    pub revision: i64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AppRelease {
    #[serde(default)]
    pub note: String,
    /// Publish: the draft revision checked; roll back: the version to restore.
    pub revision: i64,
}

pub(super) async fn files(
    tx: &DatabaseTransaction,
    w: &str,
    id: &str,
    version: i32,
) -> Result<Vec<(String, String)>> {
    rows(tx, "SELECT path,content FROM relay_built_app_files WHERE workspace=$1 AND app=$2 AND version=$3 ORDER BY path", vec![w.into(), id.into(), version.into()]).await?.into_iter().map(|r| Ok((r.try_get("", "path")?, r.try_get("", "content")?))).collect()
}

impl PgStore {
    /// A version's file list (0 is the draft), for its builder and admins.
    pub(crate) async fn built_app_files(
        &self,
        user: &User,
        w: &str,
        id: &str,
        version: i32,
    ) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        let (found, _) = app(&tx, user, w, id).await?;
        ensure!(
            found["manages"] == true,
            "Only its builder and admins see an app's code"
        );
        let list: Vec<Value> = files(&tx, w, id, version)
            .await?
            .into_iter()
            .map(|(path, content)| json!({"path":path,"size":content.len(),"content":content}))
            .collect();
        tx.commit().await?;
        Ok(
            json!({"files":list,"draft_revision":found["draft_revision"],"checks":if version == 0 { json!(built_apps::checks(&list.iter().map(|f| (f["path"].as_str().unwrap_or("").to_owned(), f["content"].as_str().unwrap_or("").to_owned())).collect::<Vec<_>>())) } else { json!([]) }}),
        )
    }

    /// The builder changes one draft file, from the draft revision they read.
    pub(crate) async fn built_app_write(
        &self,
        user: &User,
        w: &str,
        id: &str,
        input: &AppFile,
    ) -> Result<Value> {
        built_apps::path(&input.path)?;
        ensure!(
            input
                .content
                .as_ref()
                .is_none_or(|c| c.len() <= built_apps::MAX_FILE),
            "A file is at most 256 KiB"
        );
        let tx = self.workspace_tx(w).await?;
        let (found, _) = app(&tx, user, w, id).await?;
        ensure!(
            found["builder"] == user.id.as_str(),
            "Only the app's builder changes its code"
        );
        if found["draft_revision"].as_i64() != Some(input.revision) {
            return Err(crate::identity::library::model::Conflict.into());
        }
        match &input.content {
            Some(content) => execute(&tx, "INSERT INTO relay_built_app_files(workspace,app,version,path,content) VALUES($1,$2,0,$3,$4) ON CONFLICT(workspace,app,version,path) DO UPDATE SET content=EXCLUDED.content", vec![w.into(), id.into(), input.path.clone().into(), content.clone().into()]).await?,
            None => execute(&tx, "DELETE FROM relay_built_app_files WHERE workspace=$1 AND app=$2 AND version=0 AND path=$3", vec![w.into(), id.into(), input.path.clone().into()]).await?,
        };
        let revision: i64 = query(&tx, "UPDATE relay_built_apps SET draft_revision=draft_revision+1,updated=$3 WHERE workspace=$1 AND id=$2 RETURNING draft_revision", vec![w.into(), id.into(), now().into()]).await?.context("App unavailable")?.try_get("", "draft_revision")?;
        tx.commit().await?;
        Ok(json!({"draft_revision":revision}))
    }

    /// The builder publishes the draft as a new version once every check passes.
    pub(crate) async fn built_app_publish(
        &self,
        user: &User,
        w: &str,
        id: &str,
        input: &AppRelease,
    ) -> Result<Value> {
        ensure!(
            input.note.chars().count() <= 300,
            "Keep the note under 300 characters"
        );
        let tx = self.workspace_tx(w).await?;
        let (found, _) = app(&tx, user, w, id).await?;
        ensure!(
            found["builder"] == user.id.as_str(),
            "The person building the app publishes it"
        );
        if found["draft_revision"].as_i64() != Some(input.revision) {
            return Err(crate::identity::library::model::Conflict.into());
        }
        let draft = files(&tx, w, id, 0).await?;
        let failing = built_apps::checks(&draft);
        if !failing.is_empty() {
            tx.commit().await?;
            return Ok(json!({"state":"checks_failed","checks":failing}));
        }
        let version: i32 = query(&tx, "SELECT COALESCE(max(version),0)+1 AS v FROM relay_built_app_versions WHERE workspace=$1 AND app=$2", vec![w.into(), id.into()]).await?.context("Versions")?.try_get("", "v")?;
        execute(&tx, "INSERT INTO relay_built_app_files(workspace,app,version,path,content) SELECT workspace,app,$3,path,content FROM relay_built_app_files WHERE workspace=$1 AND app=$2 AND version=0", vec![w.into(), id.into(), version.into()]).await?;
        execute(&tx, "INSERT INTO relay_built_app_versions(workspace,app,version,note,published_by,published_at) VALUES($1,$2,$3,$4,$5,$6)", vec![w.into(), id.into(), version.into(), input.note.trim().into(), user.id.clone().into(), now().into()]).await?;
        execute(
            &tx,
            "UPDATE relay_built_apps SET published=$3,updated=$4 WHERE workspace=$1 AND id=$2",
            vec![w.into(), id.into(), version.into(), now().into()],
        )
        .await?;
        audit_with_summary(
            &tx,
            &user.id,
            Some(w),
            "built_app_published",
            id,
            &format!(
                "Published \"{}\" version {version}",
                found["name"].as_str().unwrap_or("")
            ),
        )
        .await?;
        tx.commit().await?;
        Ok(json!({"state":"published","version":version}))
    }

    /// The builder or an admin switches the app back to an earlier version, or (0) takes
    /// it offline.
    pub(crate) async fn built_app_rollback(
        &self,
        user: &User,
        w: &str,
        id: &str,
        input: &AppRelease,
    ) -> Result<Value> {
        let tx = self.workspace_tx(w).await?;
        let (found, _) = app(&tx, user, w, id).await?;
        ensure!(
            found["manages"] == true,
            "Its builder or an admin rolls an app back"
        );
        let version = i32::try_from(input.revision).context("Choose a version")?;
        if version != 0 {
            ensure!(query(&tx, "SELECT 1 AS found FROM relay_built_app_versions WHERE workspace=$1 AND app=$2 AND version=$3", vec![w.into(), id.into(), version.into()]).await?.is_some(), "No such version");
        }
        execute(&tx, "UPDATE relay_built_apps SET published=NULLIF($3,0),updated=$4 WHERE workspace=$1 AND id=$2", vec![w.into(), id.into(), version.into(), now().into()]).await?;
        let name = found["name"].as_str().unwrap_or("");
        audit_with_summary(
            &tx,
            &user.id,
            Some(w),
            "built_app_rolled_back",
            id,
            &if version == 0 {
                format!("Took \"{name}\" offline")
            } else {
                format!("Switched \"{name}\" to version {version}")
            },
        )
        .await?;
        tx.commit().await?;
        Ok(json!({"version":version}))
    }

    /// The app as one sandboxed document: the published version for people who may open
    /// it, or the draft as a preview for its builder and admins. Opening is audited.
    pub(crate) async fn built_app_run(
        &self,
        user: &User,
        w: &str,
        id: &str,
        preview: bool,
    ) -> Result<Value> {
        let tx = self.workspace_tx(w).await?;
        let (found, _) = app(&tx, user, w, id).await?;
        let version = if preview {
            ensure!(
                found["manages"] == true,
                "Only its builder and admins preview a draft"
            );
            0
        } else {
            ensure!(found["opens"] == true, "This app isn't published for you");
            found["published"].as_i64().unwrap_or(0) as i32
        };
        let document = built_apps::runnable(&files(&tx, w, id, version).await?)?;
        if !preview {
            audit_with_summary(
                &tx,
                &user.id,
                Some(w),
                "built_app_opened",
                id,
                &format!("Opened \"{}\"", found["name"].as_str().unwrap_or("")),
            )
            .await?;
        }
        tx.commit().await?;
        Ok(json!({"name":found["name"],"version":version,"document":document}))
    }

    /// A version's files as a ZIP, for its builder and admins.
    pub(crate) async fn built_app_download(
        &self,
        user: &User,
        w: &str,
        id: &str,
        version: i32,
    ) -> Result<(String, Vec<u8>)> {
        let tx = self.read_tx(w).await?;
        let (found, _) = app(&tx, user, w, id).await?;
        ensure!(
            found["manages"] == true,
            "Only its builder and admins download an app's code"
        );
        let list = files(&tx, w, id, version).await?;
        tx.commit().await?;
        ensure!(!list.is_empty(), "No such version");
        let mut buffer = std::io::Cursor::new(Vec::new());
        {
            use std::io::Write;
            let mut zip = zip::ZipWriter::new(&mut buffer);
            for (path, content) in &list {
                zip.start_file(path.as_str(), zip::write::SimpleFileOptions::default())?;
                zip.write_all(content.as_bytes())?;
            }
            zip.finish()?;
        }
        let name: String = found["name"]
            .as_str()
            .unwrap_or("app")
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect();
        Ok((
            format!(
                "{name}-{}.zip",
                if version == 0 {
                    "draft".into()
                } else {
                    format!("v{version}")
                }
            ),
            buffer.into_inner(),
        ))
    }
}

use crate::storage::{Artifact, Store};
use anyhow::{Context, Result, ensure};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::SystemTime,
};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

pub const MAX_FILE_BYTES: u64 = 25 * 1024 * 1024;
pub type Snapshot = BTreeMap<PathBuf, (u64, SystemTime)>;

pub struct Files {
    pub workspace: PathBuf,
    pub archives: PathBuf,
    pub logs: PathBuf,
    store: Arc<Store>,
}

impl Files {
    pub fn new(data: &Path, store: Arc<Store>) -> Result<Self> {
        let files = Self {
            workspace: data.join("workspace"),
            archives: data.join("artifacts"),
            logs: data.join("logs"),
            store,
        };
        for path in [&files.workspace, &files.archives, &files.logs] {
            std::fs::create_dir_all(path)?;
        }
        Ok(files)
    }

    pub async fn upload(&self, name: &str, contents: &[u8]) -> Result<(Artifact, String)> {
        ensure!(
            contents.len() as u64 <= MAX_FILE_BYTES,
            "File exceeds 25 MB"
        );
        let name = safe_name(name);
        let workspace_name = format!("{}-{name}", &Uuid::new_v4().to_string()[..8]);
        let path = self.workspace.join(&workspace_name);
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .await?;
        file.write_all(contents).await?;
        file.flush().await?;
        let artifact = self.archive("", &name, "upload", &path).await?;
        Ok((artifact, format!("/workspace/{workspace_name}")))
    }

    pub async fn archive(
        &self,
        task: &str,
        name: &str,
        kind: &str,
        path: &Path,
    ) -> Result<Artifact> {
        // A regular file opened with O_NOFOLLOW prevents symlink escapes from shell-created outputs.
        let mut options = tokio::fs::OpenOptions::new();
        options.read(true).custom_flags(libc::O_NOFOLLOW);
        let mut source = options
            .open(path)
            .await
            .context("Could not open output file")?;
        let metadata = source.metadata().await?;
        ensure!(metadata.is_file(), "Output must be a regular file");
        ensure!(
            metadata.len() <= MAX_FILE_BYTES,
            "Output exceeds the 25 MB download limit"
        );
        let stored = Uuid::new_v4().to_string();
        let target = self.archives.join(&stored);
        let mut dest = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .await?;
        // Bound copying too: a file could grow between its metadata read and copying.
        let count = tokio::io::copy(
            &mut tokio::io::AsyncReadExt::take(&mut source, MAX_FILE_BYTES + 1),
            &mut dest,
        )
        .await?;
        if count > MAX_FILE_BYTES {
            tokio::fs::remove_file(target).await?;
            anyhow::bail!("Output grew beyond 25 MB");
        }
        dest.flush().await?;
        let name: String = name.chars().take(120).collect();
        self.store.add_artifact(task, &name, kind, count, &stored)
    }

    pub fn snapshot(&self) -> Result<Snapshot> {
        let mut snapshot = BTreeMap::new();
        for entry in walkdir::WalkDir::new(&self.workspace)
            .follow_links(false)
            .max_depth(12)
        {
            let entry = entry?;
            ensure!(
                snapshot.len() < 2000,
                "Workspace has too many files for this initial version"
            );
            if !entry.file_type().is_file() {
                continue;
            }
            let metadata = entry.metadata()?;
            snapshot.insert(entry.path().into(), (metadata.len(), metadata.modified()?));
        }
        Ok(snapshot)
    }

    pub async fn archive_changes(
        &self,
        task: &str,
        before: &Snapshot,
    ) -> Result<(Vec<Artifact>, usize)> {
        let mut artifacts = vec![];
        let mut omitted = 0;
        for (path, stamp) in self.snapshot()? {
            if before.get(&path) == Some(&stamp) {
                continue;
            }
            if stamp.0 > MAX_FILE_BYTES || artifacts.len() == 20 {
                omitted += 1;
                continue;
            }
            let relative = path.strip_prefix(&self.workspace)?.to_string_lossy();
            artifacts.push(self.archive(task, &relative, "output", &path).await?);
        }
        Ok((artifacts, omitted))
    }
}

fn safe_name(name: &str) -> String {
    let name: String = name
        .chars()
        .take(100)
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if name.is_empty() || name == "." || name == ".." {
        "attachment".into()
    } else {
        name
    }
}

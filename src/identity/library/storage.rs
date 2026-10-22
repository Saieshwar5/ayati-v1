use super::model::{MAX_FILE, checksum};
use anyhow::{Context, Result, ensure};
use axum::body::Bytes;
use object_store::{
    ObjectStore, ObjectStoreExt, PutMode, PutOptions, aws::AmazonS3Builder,
    path::Path as ObjectPath,
};
use std::{path::PathBuf, sync::Arc};

#[derive(Clone, Default)]
pub enum Storage {
    #[default]
    Disabled,
    Local(PathBuf),
    S3(Arc<dyn ObjectStore>),
}

impl Storage {
    pub fn from_env() -> Result<Self> {
        let local = std::env::var_os("CUCKOO_LIBRARY_LOCAL_ROOT");
        let bucket = std::env::var("CUCKOO_LIBRARY_S3_BUCKET")
            .ok()
            .filter(|s| !s.is_empty());
        ensure!(
            local.is_none() || bucket.is_none(),
            "Configure only one Library storage backend"
        );
        if let Some(bucket) = bucket {
            let mut builder = AmazonS3Builder::from_env()
                .with_bucket_name(bucket)
                .with_allow_http(false);
            if let Some(endpoint) = std::env::var("CUCKOO_LIBRARY_S3_ENDPOINT")
                .ok()
                .filter(|s| !s.is_empty())
            {
                let url = reqwest::Url::parse(&endpoint).context("Invalid Library S3 endpoint")?;
                ensure!(
                    url.scheme() == "https"
                        || (url.scheme() == "http"
                            && matches!(url.host_str(), Some("127.0.0.1" | "localhost"))),
                    "Library storage requires HTTPS except loopback tests"
                );
                builder = builder
                    .with_allow_http(url.scheme() == "http")
                    .with_endpoint(endpoint);
            }
            return builder.build().map(|s| Self::S3(Arc::new(s))).map_err(|_| {
                anyhow::anyhow!(
                    "Cannot configure Library S3 storage; check private server settings"
                )
            });
        }
        if let Some(root) = local {
            let root = PathBuf::from(root);
            ensure!(
                root.is_absolute(),
                "CUCKOO_LIBRARY_LOCAL_ROOT must be an absolute development storage path"
            );
            std::fs::create_dir_all(&root)?;
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700))?;
            return Ok(Self::Local(root));
        }
        Ok(Self::Disabled)
    }
    pub fn mode(&self) -> &'static str {
        match self {
            Self::Disabled => "unconfigured",
            Self::Local(_) => "local",
            Self::S3(_) => "s3",
        }
    }
    pub fn enabled(&self) -> bool {
        !matches!(self, Self::Disabled)
    }
    fn path(&self, key: &str) -> Result<PathBuf> {
        ensure!(
            key.split('/').all(|p| !p.is_empty()
                && p != "."
                && p != ".."
                && p.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')),
            "Invalid object key"
        );
        match self {
            Self::Local(root) => Ok(root.join(key)),
            _ => anyhow::bail!("Local storage unavailable"),
        }
    }
    pub async fn put(&self, key: &str, bytes: Bytes) -> Result<()> {
        ensure!(
            !bytes.is_empty() && bytes.len() <= MAX_FILE,
            "File exceeds Library size limit"
        );
        match self {
            Self::Disabled => {
                anyhow::bail!("File storage is not configured. Pages remain available.")
            }
            Self::S3(store) => {
                let result = store
                    .put_opts(
                        &ObjectPath::from(key),
                        bytes.clone().into(),
                        PutOptions {
                            mode: PutMode::Create,
                            ..Default::default()
                        },
                    )
                    .await;
                if let Err(error) = result {
                    if !matches!(error, object_store::Error::AlreadyExists { .. }) {
                        anyhow::bail!(
                            "Storage upload failed. Retry the same file to reconcile its status."
                        );
                    }
                    ensure!(
                        self.get(key).await? == bytes,
                        "Stored upload differs from the selected file"
                    );
                }
            }
            Self::Local(_) => {
                let path = self.path(key)?;
                let parent = path.parent().context("Missing object parent")?;
                tokio::fs::create_dir_all(parent).await?;
                use std::os::unix::fs::PermissionsExt;
                tokio::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700)).await?;
                // Publish only a complete fsynced file; a crash must never leave a
                // partial canonical object that prevents an exact retry.
                let temporary = parent.join(format!("{}.pending", uuid::Uuid::new_v4()));
                let outcome = async {
                    let mut file = tokio::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .mode(0o600)
                        .open(&temporary)
                        .await?;
                    use tokio::io::AsyncWriteExt;
                    file.write_all(&bytes).await?;
                    file.sync_all().await?;
                    match tokio::fs::hard_link(&temporary, &path).await {
                        Ok(()) => {}
                        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                            ensure!(
                                self.get(key).await? == bytes,
                                "Stored upload differs from the selected file"
                            );
                        }
                        Err(error) => return Err(error.into()),
                    }
                    std::fs::File::open(parent)?.sync_all()?;
                    Ok::<_, anyhow::Error>(())
                }
                .await;
                let _ = tokio::fs::remove_file(temporary).await;
                outcome?;
            }
        }
        // Verify the bytes that will be read, not merely the upload response.
        ensure!(
            checksum(&self.get(key).await?) == checksum(&bytes),
            "Stored file failed its integrity check"
        );
        Ok(())
    }
    pub async fn get(&self, key: &str) -> Result<Bytes> {
        match self {
            Self::Disabled => anyhow::bail!("File storage is not configured"),
            Self::Local(_) => {
                let path = self.path(key)?;
                ensure!(
                    tokio::fs::metadata(&path).await?.len() <= MAX_FILE as u64,
                    "Stored file exceeds size limit"
                );
                Ok(tokio::fs::read(path).await?.into())
            }
            Self::S3(store) => {
                let result = store
                    .get(&ObjectPath::from(key))
                    .await
                    .map_err(|_| anyhow::anyhow!("Stored file unavailable; try again shortly"))?;
                ensure!(
                    result.meta.size <= MAX_FILE as u64,
                    "Stored file exceeds size limit"
                );
                result
                    .bytes()
                    .await
                    .map_err(|_| anyhow::anyhow!("Stored file transfer interrupted"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    async fn roundtrip(storage: Storage) {
        let key = format!("library/fixture/{}", uuid::Uuid::new_v4());
        let bytes = Bytes::from_static(b"Immutable Library reference");
        storage.put(&key, bytes.clone()).await.unwrap();
        storage.put(&key, bytes.clone()).await.unwrap();
        assert!(
            storage
                .put(&key, Bytes::from_static(b"Changed bytes"))
                .await
                .is_err()
        );
        assert_eq!(storage.get(&key).await.unwrap(), bytes);
    }
    #[tokio::test]
    async fn local_objects_publish_atomically_and_cannot_be_overwritten() {
        let root = tempfile::tempdir().unwrap();
        let storage = Storage::Local(root.path().into());
        assert!(storage.get("../../outside").await.is_err());
        roundtrip(storage).await;
    }
    #[tokio::test]
    #[ignore = "requires dedicated loopback S3 emulator on port 8895"]
    async fn s3_compatible_objects_support_conditional_creation_and_integrity() {
        let store = AmazonS3Builder::new()
            .with_endpoint("http://127.0.0.1:8895")
            .with_allow_http(true)
            .with_region("us-east-1")
            .with_bucket_name("cuckoo-library-fixture")
            .with_access_key_id("fixture")
            .with_secret_access_key("fixture")
            .build()
            .unwrap();
        roundtrip(Storage::S3(Arc::new(store))).await;
    }
}

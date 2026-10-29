//! Cold storage for operational history (D226): gzip JSON Lines objects that
//! Athena can query. Writing an object completes before the database removes
//! the rows, so a crash between the two at worst archives a batch twice.
use anyhow::{Context, Result, ensure};
use object_store::{
    ObjectStore, PutOptions, PutPayload, aws::AmazonS3Builder, path::Path as ObjectPath,
};
use serde_json::Value;
use std::{io::Write, path::PathBuf, sync::Arc};

pub(crate) enum Archive {
    Local(PathBuf),
    S3(Arc<dyn ObjectStore>),
}

impl Archive {
    /// None when no archive is configured: attempt history then stays put.
    pub(crate) fn from_settings(settings: &crate::config::Archive) -> Result<Option<Self>> {
        if let Some(bucket) = settings.s3_bucket.clone() {
            let mut builder = AmazonS3Builder::from_env()
                .with_bucket_name(bucket)
                .with_allow_http(false);
            if let Some(endpoint) = settings.s3_endpoint.clone() {
                let url = reqwest::Url::parse(&endpoint).context("Invalid archive S3 endpoint")?;
                ensure!(
                    url.scheme() == "https"
                        || (url.scheme() == "http"
                            && matches!(url.host_str(), Some("127.0.0.1" | "localhost"))),
                    "Archive storage requires HTTPS except loopback tests"
                );
                builder = builder
                    .with_allow_http(url.scheme() == "http")
                    .with_endpoint(endpoint);
            }
            let store = builder
                .build()
                .map_err(|_| anyhow::anyhow!("Cannot configure archive S3 storage"))?;
            return Ok(Some(Self::S3(Arc::new(store))));
        }
        Ok(settings.local_root.clone().map(Self::Local))
    }

    /// Store `records` as one gzip JSON Lines object at `key` (relative, `/`-separated).
    pub(crate) async fn put(&self, key: &str, records: &[Value]) -> Result<()> {
        ensure!(
            !key.is_empty() && !key.split('/').any(|part| part.is_empty() || part == ".."),
            "Invalid archive key"
        );
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        for record in records {
            serde_json::to_writer(&mut encoder, record)?;
            encoder.write_all(b"\n")?;
        }
        let bytes = encoder.finish()?;
        match self {
            Self::S3(store) => {
                store
                    .put_opts(
                        &ObjectPath::from(key),
                        PutPayload::from(bytes),
                        PutOptions::default(),
                    )
                    .await
                    .context("Archive upload failed")?;
            }
            Self::Local(root) => {
                let path = root.join(key);
                let parent = path.parent().context("Invalid archive key")?;
                tokio::fs::create_dir_all(parent).await?;
                // Write then rename, so a file is never seen half-written.
                let partial = path.with_extension("partial");
                tokio::fs::write(&partial, &bytes).await?;
                tokio::fs::rename(&partial, &path).await?;
            }
        }
        Ok(())
    }
}

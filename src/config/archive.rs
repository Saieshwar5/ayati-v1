//! Cold storage for operational history (D226): a private S3 bucket in the
//! cloud or an absolute local directory in development. Without one, attempt
//! history stays in the database: nothing is removed unless archived first.
use super::Reader;
use std::path::PathBuf;

#[derive(Clone, Default)]
pub struct Archive {
    pub local_root: Option<PathBuf>,
    pub s3_bucket: Option<String>,
    pub s3_endpoint: Option<String>,
    /// Days attempt diagnostics stay in the database before being archived.
    pub attempt_days: i64,
}

impl Archive {
    pub(crate) fn read(env: &mut Reader, hosted: bool) -> Self {
        let archive = Self {
            local_root: env.text("AYATI_ARCHIVE_LOCAL_ROOT").map(PathBuf::from),
            s3_bucket: env.text("AYATI_ARCHIVE_S3_BUCKET"),
            s3_endpoint: env.text("AYATI_ARCHIVE_S3_ENDPOINT"),
            attempt_days: env.number("AYATI_RETAIN_ATTEMPT_DAYS", 90, 1..=3650),
        };
        if archive.local_root.is_some() && archive.s3_bucket.is_some() {
            env.problem(
                "Configure only one archive: AYATI_ARCHIVE_LOCAL_ROOT or AYATI_ARCHIVE_S3_BUCKET",
            );
        }
        if archive
            .local_root
            .as_ref()
            .is_some_and(|root| !root.is_absolute())
        {
            env.problem("AYATI_ARCHIVE_LOCAL_ROOT must be an absolute path");
        }
        if hosted && archive.local_root.is_some() {
            env.problem("Hosted archives require private S3, not AYATI_ARCHIVE_LOCAL_ROOT");
        }
        archive
    }
    pub fn configured(&self) -> bool {
        self.local_root.is_some() || self.s3_bucket.is_some()
    }
}

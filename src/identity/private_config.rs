use anyhow::{Context, Result, ensure};
use std::path::Path;

pub(crate) fn read(path: &Path) -> Result<String> {
    use std::os::unix::fs::MetadataExt;
    ensure!(
        path.is_absolute(),
        "Use an absolute private configuration path"
    );
    let meta = std::fs::symlink_metadata(path).context("Private configuration is unavailable")?;
    ensure!(
        meta.is_file()
            && meta.mode() & 0o777 == 0o600
            && meta.uid() == unsafe { libc::geteuid() }
            && meta.len() <= 65536,
        "Private configuration must be an owned 0600 file under 64 KiB"
    );
    std::fs::read_to_string(path).context("Private configuration is unreadable")
}

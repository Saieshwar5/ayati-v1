//! Exclusive data-directory locks so two processes never share one local state root.
use anyhow::{Context, Result};
use std::path::Path;

pub(super) fn exclusive(path: &Path) -> Result<std::fs::File> {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)?;
    file.try_lock()
        .with_context(|| format!("Another Cuckoo process holds {}", path.display()))?;
    Ok(file)
}

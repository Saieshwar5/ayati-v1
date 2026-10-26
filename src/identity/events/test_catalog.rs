//! Real protected files for concurrent database tests, without mutating process env.
use super::catalog::Source;
use anyhow::Result;
use std::{
    path::PathBuf,
    sync::{Mutex, OnceLock},
};
struct Catalog {
    root: tempfile::TempDir,
    sources: Mutex<Vec<Source>>,
}
static CATALOG: OnceLock<Catalog> = OnceLock::new();
pub(crate) fn path() -> Option<PathBuf> {
    CATALOG.get().map(|c| c.root.path().join("sources.json"))
}
pub(crate) fn register(mut source: Source) -> Result<Source> {
    use std::os::unix::fs::PermissionsExt;
    let catalog = CATALOG.get_or_init(|| Catalog {
        root: tempfile::tempdir().unwrap(),
        sources: Mutex::new(vec![]),
    });
    let secret = catalog.root.path().join(format!("{}.secret", source.id));
    std::fs::write(&secret, "PRIVATE_EVENT_TEST_SIGNING_SECRET")?;
    std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o600))?;
    source.secret_file = secret.to_string_lossy().into_owned();
    let mut all = catalog.sources.lock().unwrap();
    if let Some(old) = all
        .iter_mut()
        .find(|old| old.workspace == source.workspace && old.id == source.id)
    {
        *old = source.clone();
    } else {
        all.push(source.clone());
    }
    let file = tempfile::NamedTempFile::new_in(catalog.root.path())?;
    std::fs::write(file.path(), serde_json::to_vec(&*all)?)?;
    std::fs::set_permissions(file.path(), std::fs::Permissions::from_mode(0o600))?;
    file.persist(catalog.root.path().join("sources.json"))?;
    Ok(source)
}

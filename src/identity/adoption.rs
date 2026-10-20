//! Explicit, local-only assignment of an existing installation to one workspace.
use super::*;
use anyhow::ensure;
use rusqlite::{OptionalExtension, params};
use std::{fs, path::PathBuf};

fn schema(db: &rusqlite::Connection) -> Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS installation_binding(
        id INTEGER PRIMARY KEY CHECK(id=1), workspace TEXT NOT NULL UNIQUE,
        owner TEXT NOT NULL, backup TEXT NOT NULL);",
    )?;
    Ok(())
}
pub fn has_binding(data: &Path) -> Result<bool> {
    let path = data.join("identity/identity.sqlite");
    if !path.exists() {
        return Ok(false);
    }
    let db =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let exists: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='installation_binding')",
        [],
        |r| r.get(0),
    )?;
    Ok(exists
        && db.query_row(
            "SELECT EXISTS(SELECT 1 FROM installation_binding)",
            [],
            |r| r.get(0),
        )?)
}

pub fn release(data: &Path, workspace: &str) -> Result<()> {
    let data = data.canonicalize()?;
    let _daemon = lock(&data.join("daemon.lock"))?;
    let _identity = lock(&data.join("identity/server.lock"))?;
    let store = Store::open(&data.join("identity/identity.sqlite"))?;
    let bound = binding(&store)?.context("No installation binding exists")?;
    ensure!(
        bound.0 == workspace,
        "Workspace does not own this installation"
    );
    store
        .0
        .lock()
        .unwrap()
        .execute("DELETE FROM installation_binding WHERE id=1", [])?;
    println!("Installation binding released; business records remain in their original location.");
    Ok(())
}

pub(super) fn binding(store: &Store) -> Result<Option<(String, String)>> {
    let db = store.0.lock().unwrap();
    schema(&db)?;
    Ok(db
        .query_row(
            "SELECT workspace,owner FROM installation_binding WHERE id=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?)
}

pub(super) fn lock(path: &Path) -> Result<std::fs::File> {
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)?;
    file.try_lock()
        .context("Stop both local servers before changing installation ownership")?;
    Ok(file)
}

/// Invoked on the host, never exposed as an HTTP claim-installation endpoint.
pub fn adopt(data: &Path, workspace: &str) -> Result<()> {
    ensure!(
        uuid::Uuid::parse_str(workspace).is_ok(),
        "Invalid workspace ID"
    );
    let data = data.canonicalize()?;
    let _daemon = lock(&data.join("daemon.lock"))?;
    let _identity = lock(&data.join("identity/server.lock"))?;
    let store = Store::open(&data.join("identity/identity.sqlite"))?;
    if let Some((bound, _)) = binding(&store)? {
        ensure!(
            bound == workspace,
            "Installation is already assigned to another workspace"
        );
        println!("Installation is already assigned to this workspace.");
        return Ok(());
    }
    let owner: String = {
        let db = store.0.lock().unwrap();
        let owners = db
            .prepare("SELECT user FROM members WHERE workspace=?1 AND role='Owner'")?
            .query_map([workspace], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        ensure!(owners.len() == 1, "Workspace must have exactly one owner");
        owners[0].clone()
    };
    ensure!(
        data.join("ayati.sqlite").is_file(),
        "No existing installation found"
    );
    let mut vm_locks = Vec::new();
    if data.join("vms").exists() {
        lock_vms(&data.join("vms"), &mut vm_locks)?;
    }
    ensure_quiet(&data.join("ayati.sqlite"))?;
    if data.join("agents").exists() {
        for agent in fs::read_dir(data.join("agents"))? {
            let db = agent?.path().join("ayati.sqlite");
            if db.exists() {
                ensure_quiet(&db)?;
            }
        }
    }
    // Binding never replaces either store. Refuse to hide newly created business data.
    let destination = data.join("identity/workspaces").join(workspace);
    ensure_empty(&destination)?;
    let backup = data
        .join("backups")
        .join(format!("before-workspace-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&backup)?;
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&backup, fs::Permissions::from_mode(0o700))?;
    for entry in fs::read_dir(&data)? {
        let entry = entry?;
        if entry.file_name() == "backups" {
            continue;
        }
        // Reflinks preserve large disks without changing their paths or source bytes.
        let copied = std::process::Command::new("cp")
            .args(["-a", "--reflink=auto"])
            .arg(entry.path())
            .arg(&backup)
            .status()?;
        ensure!(
            copied.success(),
            "Backup failed; no ownership change was made"
        );
    }
    verify_databases(&backup)?;
    let db = store.0.lock().unwrap();
    db.execute(
        "INSERT INTO installation_binding VALUES(1,?1,?2,?3)",
        params![workspace, owner, backup.to_string_lossy()],
    )?;
    println!(
        "Existing installation assigned. Verified backup: {}",
        backup.display()
    );
    Ok(())
}

fn ensure_empty(root: &Path) -> Result<()> {
    for (file, tables) in [
        ("ayati.sqlite", &["tasks", "messages", "artifacts"][..]),
        ("agents.sqlite", &["agents"][..]),
        ("business/platform.sqlite", &["apps", "panels"][..]),
    ] {
        let path = root.join(file);
        if !path.exists() {
            continue;
        }
        let db = rusqlite::Connection::open_with_flags(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        for table in tables {
            let count: i64 =
                db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))?;
            ensure!(
                count == 0,
                "Selected workspace already has business data; reconcile it before adoption"
            );
        }
    }
    Ok(())
}
fn ensure_quiet(path: &Path) -> Result<()> {
    let db =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let active: i64 = db.query_row(
        "SELECT count(*) FROM tasks WHERE status IN ('queued','running')",
        [],
        |r| r.get(0),
    )?;
    let schedules: i64 = db.query_row(
        "SELECT count(*) FROM schedules WHERE state='active'",
        [],
        |r| r.get(0),
    )?;
    ensure!(
        active == 0 && schedules == 0,
        "Stop queued work and pause schedules before adopting this installation"
    );
    Ok(())
}
fn lock_vms(root: &Path, locks: &mut Vec<fs::File>) -> Result<()> {
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if path.is_symlink() {
            continue;
        }
        if path.is_dir() {
            lock_vms(&path, locks)?;
        } else if path.file_name().is_some_and(|name| name == "vm.lock") {
            locks.push(lock(&path)?);
        }
    }
    Ok(())
}

fn verify_databases(root: &Path) -> Result<()> {
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if path.is_symlink() {
            continue;
        }
        if path.is_dir() {
            verify_databases(&path)?;
        } else if path.extension().is_some_and(|ext| ext == "sqlite") {
            let db = rusqlite::Connection::open(&path)?;
            let integrity: String = db.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
            ensure!(integrity == "ok", "Backup integrity check failed");
        }
    }
    Ok(())
}

pub(super) fn resolve(root: &Path, bound: &Option<(String, String)>, id: &str) -> PathBuf {
    if bound.as_ref().is_some_and(|(workspace, _)| workspace == id) {
        root.parent().unwrap().parent().unwrap().to_owned()
    } else {
        root.join(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn binding_is_explicit_and_does_not_change_other_workspace_paths() {
        let root = Path::new("/data/identity/workspaces");
        assert_eq!(resolve(root, &None, "first"), root.join("first"));
        let bound = Some(("first".into(), "owner".into()));
        assert_eq!(resolve(root, &bound, "first"), Path::new("/data"));
        assert_eq!(resolve(root, &bound, "second"), root.join("second"));
    }
    #[test]
    fn existing_destination_data_prevents_adoption() {
        let temp = tempfile::tempdir().unwrap();
        let db = rusqlite::Connection::open(temp.path().join("agents.sqlite")).unwrap();
        db.execute_batch("CREATE TABLE agents(id TEXT); INSERT INTO agents VALUES('existing')")
            .unwrap();
        assert!(ensure_empty(temp.path()).is_err());
    }
    #[test]
    fn adoption_backs_up_data_is_idempotent_and_can_be_released() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir(root.join("identity")).unwrap();
        let legacy = crate::storage::Store::open(&root.join("ayati.sqlite")).unwrap();
        legacy
            .accept_input("saved original conversation", None)
            .unwrap();
        drop(legacy);
        rusqlite::Connection::open(root.join("ayati.sqlite"))
            .unwrap()
            .execute("UPDATE tasks SET status='completed'", [])
            .unwrap();
        let store = Store::open(&root.join("identity/identity.sqlite")).unwrap();
        let owner = store
            .login("owner", "owner@example.com", "hash", "sid")
            .unwrap();
        let workspace = uuid::Uuid::new_v4().to_string();
        store
            .create(&owner, "Existing business", &workspace)
            .unwrap();
        drop(store);
        adopt(root, &workspace).unwrap();
        adopt(root, &workspace).unwrap();
        assert!(has_binding(root).unwrap());
        let store = Store::open(&root.join("identity/identity.sqlite")).unwrap();
        let backup: String = store
            .0
            .lock()
            .unwrap()
            .query_row("SELECT backup FROM installation_binding", [], |r| r.get(0))
            .unwrap();
        let restored =
            crate::storage::Store::open(&PathBuf::from(backup).join("ayati.sqlite")).unwrap();
        assert_eq!(
            restored.messages().unwrap()[0].content,
            "saved original conversation"
        );
        drop(restored);
        drop(store);
        release(root, &workspace).unwrap();
        assert!(!has_binding(root).unwrap());
        assert_eq!(
            crate::storage::Store::open(&root.join("ayati.sqlite"))
                .unwrap()
                .messages()
                .unwrap()[0]
                .content,
            "saved original conversation"
        );
    }
}

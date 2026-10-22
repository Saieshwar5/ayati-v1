use super::*;
use rusqlite::Connection;
fn fixture() -> (Store, User, User, String) {
    let db = Store::open(Path::new(":memory:")).unwrap();
    let a = db
        .login("user_owner", "owner@example.test", "a", "a")
        .unwrap();
    let b = db
        .login("user_second", "second@example.test", "b", "b")
        .unwrap();
    let w = db
        .create(&a, "Workspace", &uuid::Uuid::new_v4().to_string())
        .unwrap();
    let invite = db.reserve_invite(&a, &w, &b.email, "User").unwrap();
    db.delivered(&invite, Some("test")).unwrap();
    db.accept(&b, &invite).unwrap();
    (db, a, b, w)
}
#[test]
fn only_owners_manage_settings_and_can_appoint_equal_owners() {
    let (db, a, b, w) = fixture();
    assert!(db.people(&b, &w).is_err());
    assert!(db.workspace_settings(&b, &w).is_err());
    assert!(db.workspace_activity(&b, &w).is_err());
    assert!(db.rename_workspace(&b, &w, "Changed", "Workspace").is_err());
    assert!(db.change(&b, &w, &b.id, Some("Owner")).is_err());
    assert!(
        db.reserve_invite(&a, &w, "new@example.test", "Owner")
            .is_err()
    );
    db.change(&a, &w, &b.id, Some("Owner")).unwrap();
    db.rename_workspace(&b, &w, "Renamed", "Workspace").unwrap();
    assert!(db.rename_workspace(&a, &w, "Stale", "Workspace").is_err());
    db.change(&b, &w, &a.id, Some("User")).unwrap();
    assert!(db.workspace_settings(&a, &w).is_err());
    assert!(db.change(&b, &w, &b.id, None).is_err());
    assert!(db.change(&b, &w, &b.id, Some("User")).is_err());
    db.change(&b, &w, &a.id, None).unwrap();
    assert!(db.role(&w, &a.id).is_err());
    assert!(db.accept(&a, "not-an-invitation").is_err());
    assert_eq!(db.workspace_settings(&b, &w).unwrap()["name"], "Renamed");
}
#[test]
fn sqlite_role_migration_retains_dependent_access_and_never_promotes_admins() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("identity.sqlite");
    let db = Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON;
        CREATE TABLE users(id TEXT PRIMARY KEY,provider TEXT UNIQUE NOT NULL,email TEXT NOT NULL);
        INSERT INTO users VALUES('owner','user_owner','owner@test'),('admin','user_admin','admin@test');
        CREATE TABLE workspaces(id TEXT PRIMARY KEY,name TEXT NOT NULL);
        INSERT INTO workspaces VALUES('w','Existing');
        CREATE TABLE members(workspace TEXT REFERENCES workspaces(id),user TEXT REFERENCES users(id),role TEXT CHECK(role IN ('Owner','Admin','Member')),PRIMARY KEY(workspace,user));
        INSERT INTO members VALUES('w','owner','Owner'),('w','admin','Admin');
        CREATE TABLE retained(workspace TEXT,user TEXT,FOREIGN KEY(workspace,user) REFERENCES members(workspace,user));
        INSERT INTO retained VALUES('w','admin');").unwrap();
    drop(db);
    let db = Store::open(&path).unwrap();
    assert_eq!(db.role("w", "admin").unwrap(), "User");
    assert_eq!(db.role("w", "owner").unwrap(), "Owner");
    assert_eq!(
        db.0.lock()
            .unwrap()
            .query_row("SELECT count(*) FROM retained", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert!(
        db.0.lock()
            .unwrap()
            .execute("INSERT INTO members VALUES('w','other','Admin')", [])
            .is_err()
    );
    drop(db);
    assert_eq!(
        Store::open(&path).unwrap().role("w", "admin").unwrap(),
        "User"
    );
}

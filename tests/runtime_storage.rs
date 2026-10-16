use ayati::storage::Store;
use rusqlite::Connection;
use serde_json::json;

#[test]
fn version_one_history_survives_migration_and_new_runs_have_scope() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(include_str!("../src/storage/schema.sql"))
        .unwrap();
    connection.execute_batch("PRAGMA user_version=1; INSERT INTO tasks(id,status,context) VALUES('saved','completed','[]'); INSERT INTO messages(id,task_id,role,content,state) VALUES('original','saved','user','Preserve this','complete');").unwrap();
    drop(connection);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.messages().unwrap()[0].content, "Preserve this");
    let task = store.enqueue("New work").unwrap();
    let connection = Connection::open(&path).unwrap();
    assert_eq!(
        connection
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        9
    );
    let scope: (String, String) = connection.query_row(
        "SELECT a.business_id,a.id FROM run_scopes s JOIN conversations c ON c.id=s.conversation_id JOIN agents a ON a.id=c.agent_id WHERE s.task_id=?1",
        [&task.id], |r| Ok((r.get(0)?, r.get(1)?)),
    ).unwrap();
    assert_eq!(scope, ("local-business".into(), "local-agent".into()));
}

#[test]
fn approval_is_immutable_single_use_and_cancelled_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    let store = Store::open(&path).unwrap();
    let task = store.enqueue("Demo action").unwrap();
    store.set_task(&task.id, "running", "Waiting").unwrap();
    let action = json!({"tool":"proof", "arguments":{}});
    let allowed = store
        .add_permission(&task.id, "Read demo", &action)
        .unwrap();
    store.decide_permission(&allowed, true).unwrap();
    assert!(store.decide_permission(&allowed, false).is_err());
    let pending = store
        .add_permission(&task.id, "Another read", &action)
        .unwrap();
    drop(store);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.permission_decision(&pending).unwrap(), "cancelled");
    assert!(store.decide_permission(&pending, true).is_err());
    assert_eq!(store.permission_decision(&allowed).unwrap(), "allowed");
    assert_eq!(store.task(&task.id).unwrap().unwrap().status, "interrupted");
}

#[test]
fn newer_database_is_rejected_without_modification() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    let connection = Connection::open(&path).unwrap();
    connection.execute_batch("PRAGMA user_version=10").unwrap();
    assert!(Store::open(&path).is_err());
    assert_eq!(
        connection
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        10
    );
}

#[test]
fn changed_runtime_settings_cannot_reuse_a_session_with_old_tools() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("state.sqlite")).unwrap();
    store
        .save_runtime_session("saved-session", "proof-tool-enabled")
        .unwrap();
    assert_eq!(
        store
            .runtime_session("proof-tool-enabled")
            .unwrap()
            .as_deref(),
        Some("saved-session")
    );
    assert!(store.runtime_session("proof-tool-disabled").is_err());
    assert_eq!(
        store
            .runtime_session("proof-tool-enabled")
            .unwrap()
            .as_deref(),
        Some("saved-session")
    );
}

use ayati::storage::{Store, profile::UpdateProfile};

fn edit(name: &str, role: &str, revision: i64) -> UpdateProfile {
    UpdateProfile {
        model: None,
        memory: None,
        browser: None,
        name: name.into(),
        role: role.into(),
        expected_revision: revision,
    }
}

#[test]
fn profile_revision_and_context_reset_are_atomic_and_durable() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    let store = Store::open(&path).unwrap();
    store.save_runtime_session("original", "settings").unwrap();
    let renamed = store
        .update_profile(&edit("New name", "", 0), "", "fixture", false, false)
        .unwrap();
    assert_eq!(renamed.role_revision, 0);
    assert_eq!(
        store.runtime_session("settings").unwrap().as_deref(),
        Some("original")
    );
    let task = store.enqueue("preserve history").unwrap();
    assert!(
        store
            .update_profile(&edit("Changed", "New role", 1), "", "fixture", false, false)
            .is_err()
    );
    store.set_task(&task.id, "running", "").unwrap();
    store.set_task(&task.id, "completed", "").unwrap();
    let changed = store
        .update_profile(&edit("Changed", "New role", 1), "", "fixture", false, false)
        .unwrap();
    assert_eq!(changed.role_revision, 1);
    assert!(store.runtime_session("settings").unwrap().is_none());
    assert!(
        store
            .update_profile(&edit("Stale", "Old role", 1), "", "fixture", false, false)
            .is_err()
    );
    assert_eq!(store.profile_edit().unwrap().unwrap().name, "Changed");
    drop(store);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.profile_edit().unwrap().unwrap().revision, 2);
    assert_eq!(store.messages().unwrap()[0].content, "preserve history");
    let other = Store::open(&dir.path().join("other.sqlite")).unwrap();
    assert!(other.profile_edit().unwrap().is_none());
}

#[test]
fn invalid_input_and_pending_approvals_cannot_change_profile_or_context() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("state.sqlite")).unwrap();
    store.save_runtime_session("original", "settings").unwrap();
    for input in [
        edit(" ", "role", 0),
        edit("name", "\0", 0),
        edit(&"n".repeat(81), "role", 0),
        edit("name", &"r".repeat(4001), 0),
        edit("name", "role", -1),
    ] {
        assert!(
            store
                .update_profile(&input, "", "fixture", false, false)
                .is_err()
        );
    }
    let task = store.enqueue("approval").unwrap();
    store.set_task(&task.id, "running", "").unwrap();
    store
        .add_permission(&task.id, "demo", &serde_json::json!({}))
        .unwrap();
    assert!(
        store
            .update_profile(&edit("Name", "Role", 0), "", "fixture", false, false)
            .is_err()
    );
    assert!(store.profile_edit().unwrap().is_none());
    assert_eq!(
        store.runtime_session("settings").unwrap().as_deref(),
        Some("original")
    );
}

#[test]
fn failed_session_reset_rolls_back_the_entire_profile_edit() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    let store = Store::open(&path).unwrap();
    store.save_runtime_session("original", "settings").unwrap();
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection.execute_batch("CREATE TRIGGER reject_session_reset BEFORE DELETE ON runtime_sessions BEGIN SELECT RAISE(ABORT,'simulated storage failure'); END;").unwrap();
    assert!(
        store
            .update_profile(&edit("Name", "New role", 0), "", "fixture", false, false)
            .is_err()
    );
    assert!(store.profile_edit().unwrap().is_none());
    assert_eq!(
        store.runtime_session("settings").unwrap().as_deref(),
        Some("original")
    );
    connection
        .execute_batch("DROP TRIGGER reject_session_reset;")
        .unwrap();
    assert_eq!(
        store
            .update_profile(&edit("Name", "New role", 0), "", "fixture", false, false)
            .unwrap()
            .revision,
        1
    );
}

#[test]
fn model_edits_validate_preserve_omissions_and_reset_context_atomically() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    let store = Store::open(&path).unwrap();
    store.save_runtime_session("original", "settings").unwrap();
    let mut input = edit("Name", "", 0);
    input.model = Some("fixture".into());
    assert_eq!(
        store
            .update_profile(&input, "", "fixture", false, false)
            .unwrap()
            .role_revision,
        0
    );
    assert!(store.runtime_session("settings").unwrap().is_some());
    input.expected_revision = 1;
    for model in ["", "bad model", "bad\nmodel", &"x".repeat(201)] {
        input.model = Some(model.into());
        assert!(
            store
                .update_profile(&input, "", "fixture", false, false)
                .is_err()
        );
        assert_eq!(store.profile_edit().unwrap().unwrap().revision, 1);
    }
    input.model = Some("fixture-next".into());
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection.execute_batch("CREATE TRIGGER reject_model_reset BEFORE DELETE ON runtime_sessions BEGIN SELECT RAISE(ABORT,'failure'); END;").unwrap();
    assert!(
        store
            .update_profile(&input, "", "fixture", false, false)
            .is_err()
    );
    assert_eq!(
        store.profile_edit().unwrap().unwrap().model.as_deref(),
        Some("fixture")
    );
    connection
        .execute_batch("DROP TRIGGER reject_model_reset")
        .unwrap();
    assert_eq!(
        store
            .update_profile(&input, "", "fixture", false, false)
            .unwrap()
            .role_revision,
        1
    );
    assert!(store.runtime_session("settings").unwrap().is_none());
    let renamed = store
        .update_profile(&edit("Renamed", "", 2), "", "fixture", false, false)
        .unwrap();
    assert_eq!(renamed.model.as_deref(), Some("fixture-next"));
    assert_eq!(renamed.role_revision, 1);
    drop(store);
    assert_eq!(
        Store::open(&path)
            .unwrap()
            .profile_edit()
            .unwrap()
            .unwrap()
            .model
            .as_deref(),
        Some("fixture-next")
    );
}

#[test]
fn schema_seven_profile_and_session_survive_model_migration() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    let store = Store::open(&path).unwrap();
    store
        .update_profile(&edit("Saved", "Role", 0), "", "fixture", false, false)
        .unwrap();
    store
        .save_runtime_session("retained", "role-settings")
        .unwrap();
    drop(store);
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute_batch("ALTER TABLE profile_edit DROP COLUMN model; ALTER TABLE profile_edit DROP COLUMN memory; ALTER TABLE profile_edit DROP COLUMN browser; DROP TABLE permission_uses; PRAGMA user_version=7")
        .unwrap();
    drop(connection);
    let store = Store::open(&path).unwrap();
    let profile = store.profile_edit().unwrap().unwrap();
    assert_eq!(profile.name, "Saved");
    assert_eq!(profile.role_revision, 1);
    assert!(profile.model.is_none());
    assert_eq!(
        store.runtime_session("role-settings").unwrap().as_deref(),
        Some("retained")
    );
}

#[test]
fn tool_changes_reset_context_and_preserve_omitted_flags() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    let store = Store::open(&path).unwrap();
    store.save_runtime_session("old", "settings").unwrap();
    let mut input = edit("Agent", "", 0);
    input.memory = Some(false);
    input.browser = Some(false);
    let saved = store
        .update_profile(&input, "", "fixture", true, true)
        .unwrap();
    assert_eq!(saved.role_revision, 1);
    assert!(store.runtime_session("settings").unwrap().is_none());
    let saved = store
        .update_profile(&edit("Renamed", "", 1), "", "fixture", true, true)
        .unwrap();
    assert_eq!(saved.memory, Some(false));
    assert_eq!(saved.browser, Some(false));
    assert_eq!(saved.role_revision, 1);
    drop(store);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.profile_edit().unwrap().unwrap().browser, Some(false));
    let mut input = edit("Renamed", "", 2);
    input.memory = Some(true);
    input.browser = Some(true);
    assert_eq!(
        store
            .update_profile(&input, "", "fixture", true, true)
            .unwrap()
            .role_revision,
        2
    );
}

#[test]
fn schema_eight_model_override_survives_tool_migration() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    let store = Store::open(&path).unwrap();
    let mut input = edit("Agent", "", 0);
    input.model = Some("saved-model".into());
    store
        .update_profile(&input, "", "fixture", true, true)
        .unwrap();
    store
        .save_runtime_session("retained", "model-settings")
        .unwrap();
    drop(store);
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection.execute_batch("ALTER TABLE profile_edit DROP COLUMN memory; ALTER TABLE profile_edit DROP COLUMN browser; DROP TABLE permission_uses; PRAGMA user_version=8").unwrap();
    drop(connection);
    let store = Store::open(&path).unwrap();
    let profile = store.profile_edit().unwrap().unwrap();
    assert_eq!(profile.model.as_deref(), Some("saved-model"));
    assert!(profile.memory.is_none() && profile.browser.is_none());
    assert_eq!(profile.role_revision, 1);
    assert_eq!(
        store.runtime_session("model-settings").unwrap().as_deref(),
        Some("retained")
    );
}

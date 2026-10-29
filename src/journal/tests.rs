use super::{Journal, work::IntakeError};
use serde_json::json;

fn journal() -> (tempfile::TempDir, Journal) {
    let dir = tempfile::tempdir().unwrap();
    let store = Journal::open(&dir.path().join("chat.sqlite")).unwrap();
    (dir, store)
}

#[test]
fn a_journal_never_reopens_another_runs_file() {
    let (dir, store) = journal();
    drop(store);
    assert!(Journal::open(&dir.path().join("chat.sqlite")).is_err());
}

#[test]
fn acceptance_is_idempotent_bounded_and_rejects_changed_payloads() {
    let (_dir, store) = journal();
    let key = uuid::Uuid::new_v4().to_string();
    let first = store.accept_input("Inventory check", Some(&key)).unwrap();
    for _ in 1..32 {
        store.accept_input("Waiting job", None).unwrap();
    }
    assert_eq!(
        store
            .accept_input("Inventory check", Some(&key))
            .unwrap()
            .id,
        first.id
    );
    let error = |result: anyhow::Result<super::Task>| {
        result
            .err()
            .unwrap()
            .downcast::<IntakeError>()
            .map(|e| e.to_string())
    };
    assert!(matches!(
        error(store.accept_input("Overflow", None)).as_deref(),
        Ok("Too many pending requests; please wait or stop one")
    ));
    assert!(error(store.accept_input("Changed", Some(&key))).is_ok());
    assert!(
        store
            .accept_input("Invalid key", Some("not-an-id"))
            .is_err()
    );
}

#[test]
fn only_one_claim_runs_and_approvals_are_single_decisions() {
    let (_dir, store) = journal();
    store.accept_input("Demo", None).unwrap();
    let run = store.claim_next("owner").unwrap().unwrap();
    assert!(store.claim_next("another-owner").unwrap().is_none());
    store
        .observe_tool(&run, "call", "demo_write", &json!({"value":1}))
        .unwrap();
    let permission = store
        .add_claim_permission(&run, "call", "Demo write", &json!({"value":1}))
        .unwrap();
    store.decide_permission(&permission, true).unwrap();
    assert!(store.decide_permission(&permission, false).is_err());
}

#[test]
fn an_unconfirmed_tool_prevents_a_false_completed_outcome() {
    let (_dir, store) = journal();
    let task = store.accept_input("Demo action", None).unwrap();
    let run = store.claim_next("owner").unwrap().unwrap();
    store
        .observe_tool(&run, "call", "demo_action", &json!({}))
        .unwrap();
    store
        .observe_tool_status(&run, "call", "running", None)
        .unwrap();
    store.finish_work(&run, "completed", "").unwrap();
    assert_eq!(store.task(&task.id).unwrap().unwrap().status, "interrupted");
    let completed: i64 = store
        .connection()
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM work_transitions WHERE to_status='completed'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(completed, 0);
}

#[test]
fn changed_runtime_settings_cannot_reuse_a_session_with_old_tools() {
    let (_dir, store) = journal();
    store.save_runtime_session("saved", "tools-a").unwrap();
    assert_eq!(
        store.runtime_session("tools-a").unwrap().as_deref(),
        Some("saved")
    );
    assert!(store.runtime_session("tools-b").is_err());
}

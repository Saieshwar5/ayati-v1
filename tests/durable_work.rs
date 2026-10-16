use ayati::storage::{
    Store,
    work::{IntakeError, RunClaim},
};
use rusqlite::Connection;
use serde_json::json;

fn claim(store: &Store) -> RunClaim {
    store.claim_next("test-owner").unwrap().unwrap()
}

#[test]
fn owner_takeover_interrupts_work_and_invalidates_approval_without_replaying_or_cancelling_queue() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("state.sqlite")).unwrap();
    let active = store.enqueue("Browse a shop").unwrap();
    let claim = claim(&store);
    let queued = store.enqueue("Later work").unwrap();
    store
        .observe_tool(
            &claim,
            "browser-call",
            "ayati_browser__agent_browser_click",
            &json!({"selector":"@e1"}),
        )
        .unwrap();
    let permission = store
        .add_claim_permission(
            &claim,
            "browser-call",
            "Browser action",
            &json!({"selector":"@e1"}),
        )
        .unwrap();
    store.interrupt_for_owner_browser().unwrap();
    assert_eq!(
        store.task(&active.id).unwrap().unwrap().status,
        "interrupted"
    );
    assert!(
        store
            .task(&active.id)
            .unwrap()
            .unwrap()
            .detail
            .contains("owner browser")
    );
    assert_eq!(store.task(&queued.id).unwrap().unwrap().status, "queued");
    assert!(store.decide_permission(&permission, true).is_err());
    assert!(
        !store
            .finish_work(&claim, "completed", "Late success")
            .unwrap()
    );
}

#[test]
fn acceptance_is_atomic_idempotent_and_bounded_even_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    let key = uuid::Uuid::new_v4().to_string();
    let store = Store::open(&path).unwrap();
    let first = store.accept_input("Inventory check", Some(&key)).unwrap();
    for _ in 1..32 {
        store.enqueue("Waiting job").unwrap();
    }
    assert_eq!(
        store
            .accept_input("Inventory check", Some(&key))
            .unwrap()
            .id,
        first.id
    );
    assert!(matches!(
        store
            .enqueue("Overflow")
            .err()
            .unwrap()
            .downcast_ref::<IntakeError>(),
        Some(IntakeError::Full)
    ));
    assert!(matches!(
        store
            .accept_input("Changed", Some(&key))
            .err()
            .unwrap()
            .downcast_ref::<IntakeError>(),
        Some(IntakeError::Conflict)
    ));
    assert!(
        store
            .accept_input("Invalid key", Some("not-an-id"))
            .is_err()
    );
    drop(store);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.pending_count().unwrap(), 32);
    assert_eq!(claim(&store).task_id, first.id);
    let db = Connection::open(&path).unwrap();
    for table in ["tasks", "messages", "input_receipts", "run_scopes"] {
        assert_eq!(
            db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            32
        );
    }
}

#[test]
fn stop_is_terminal_and_stale_owners_cannot_write_replies_results_or_approvals() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("state.sqlite")).unwrap();
    let task = store.enqueue("Demo").unwrap();
    let run = claim(&store);
    assert!(store.claim_next("another-owner").unwrap().is_none());
    let reply = store.add_claim_reply(&run).unwrap();
    store
        .save_claim_reply(&run, &reply, "Before stop", "streaming")
        .unwrap();
    store
        .observe_tool(&run, "call", "demo_write", &json!({"value":1}))
        .unwrap();
    let permission = store
        .add_claim_permission(&run, "call", "Demo write", &json!({"value":1}))
        .unwrap();
    store.stop_work(&task.id).unwrap();
    assert!(store.decide_permission(&permission, true).is_err());
    assert!(
        !store
            .save_claim_reply(&run, &reply, "Late text", "complete")
            .unwrap()
    );
    assert!(!store.finish_work(&run, "completed", "").unwrap());
    assert!(
        store
            .add_claim_permission(&run, "call", "Late approval", &json!({}))
            .is_err()
    );
    store
        .observe_tool_status(&run, "call", "completed", Some(&json!({"late":true})))
        .unwrap();
    store.set_task(&task.id, "running", "Late restart").unwrap();
    assert_eq!(store.task(&task.id).unwrap().unwrap().status, "stopped");
    assert_eq!(store.messages().unwrap()[1].content, "Before stop");
    assert_eq!(store.actions(&task.id).unwrap()[0].status, "cancelled");
    assert!(store.claim_next("another-owner").unwrap().is_none());
}

#[test]
fn recovery_keeps_queued_work_and_interrupts_only_work_that_had_started() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    let store = Store::open(&path).unwrap();
    let started = store.enqueue("Already started").unwrap();
    let waiting = store.enqueue("Safe to start").unwrap();
    let cancelled = store.enqueue("Owner cancelled").unwrap();
    let old = claim(&store);
    store.stop_work(&cancelled.id).unwrap();
    drop(store);
    let store = Store::open(&path).unwrap();
    assert!(!store.claim_active(&old).unwrap());
    assert!(!store.finish_work(&old, "completed", "").unwrap());
    assert_eq!(
        store.task(&started.id).unwrap().unwrap().status,
        "interrupted"
    );
    assert_eq!(
        store.task(&cancelled.id).unwrap().unwrap().status,
        "stopped"
    );
    let next = claim(&store);
    assert_eq!(next.task_id, waiting.id);
    let mut forged = next.clone();
    forged.token = old.token;
    assert!(!store.finish_work(&forged, "completed", "").unwrap());
    assert!(store.finish_work(&next, "completed", "").unwrap());
}

#[test]
fn a_simulated_external_write_with_lost_reply_is_recorded_uncertain_and_never_replayed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    // Independent fake service state supplies outcome evidence, not an assistant claim.
    let service_receipt = dir.path().join("fake-service-receipt.json");
    let store = Store::open(&path).unwrap();
    let task = store.enqueue("Send synthetic notification").unwrap();
    let run = claim(&store);
    let input = json!({"operation_id":"demo-notification-1"});
    store
        .observe_tool(&run, "call", "demo_notification", &input)
        .unwrap();
    let permission = store
        .add_claim_permission(&run, "call", "Send demo notification", &input)
        .unwrap();
    store.decide_permission(&permission, true).unwrap();
    store
        .observe_tool_status(&run, "call", "running", None)
        .unwrap();
    std::fs::write(
        &service_receipt,
        json!({"operation_id":"demo-notification-1","deliveries":1}).to_string(),
    )
    .unwrap();
    // Simulate process loss after the service commits, before a tool result is saved.
    drop(store);
    let store = Store::open(&path).unwrap();
    let action = &store.actions(&task.id).unwrap()[0];
    assert_eq!(action.status, "uncertain");
    assert_eq!(action.decision.as_deref(), Some("allowed"));
    assert!(action.result.is_none());
    assert!(store.claim_next("restarted-owner").unwrap().is_none());
    assert!(
        store
            .observe_tool(&run, "call", "demo_notification", &input)
            .is_err()
    );
    let receipt: serde_json::Value =
        serde_json::from_slice(&std::fs::read(service_receipt).unwrap()).unwrap();
    assert_eq!(receipt["deliveries"], 1);
}

#[test]
fn version_two_migration_preserves_sessions_and_does_not_replay_legacy_queue() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    let db = Connection::open(&path).unwrap();
    db.execute_batch(include_str!("../src/storage/schema.sql"))
        .unwrap();
    db.execute_batch(include_str!("../src/storage/runtime.sql"))
        .unwrap();
    db.execute_batch("INSERT INTO tasks(id,status,context) VALUES('old-queue','queued','[]'); INSERT INTO runtime_sessions VALUES('local-conversation','saved-goose-session','1.54.0','settings');").unwrap();
    drop(db);
    let store = Store::open(&path).unwrap();
    assert_eq!(
        store.runtime_session("settings").unwrap().as_deref(),
        Some("saved-goose-session")
    );
    assert_eq!(
        store.task("old-queue").unwrap().unwrap().status,
        "interrupted"
    );
    assert!(store.claim_next("owner").unwrap().is_none());
}

#[test]
fn an_unconfirmed_tool_prevents_a_false_completed_outcome() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    let store = Store::open(&path).unwrap();
    let task = store.enqueue("Demo action").unwrap();
    let run = claim(&store);
    store
        .observe_tool(&run, "call", "demo_action", &json!({}))
        .unwrap();
    store
        .observe_tool_status(&run, "call", "running", None)
        .unwrap();
    store.finish_work(&run, "completed", "").unwrap();
    assert_eq!(store.task(&task.id).unwrap().unwrap().status, "interrupted");
    assert_eq!(store.actions(&task.id).unwrap()[0].status, "uncertain");
    let db = Connection::open(path).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM work_transitions WHERE to_status='completed'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}

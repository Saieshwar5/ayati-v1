use ayati::storage::Store;
use serde_json::json;

#[test]
fn attention_survives_recent_history_limits_and_reviews_persist() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    let store = Store::open(&path).unwrap();
    let mut failures = Vec::new();
    for _ in 0..23 {
        let task = store.enqueue(&"Original request ".repeat(50)).unwrap();
        store
            .set_task(&task.id, "failed", "Provider unavailable")
            .unwrap();
        failures.push(task.id);
    }
    for _ in 0..110 {
        let task = store.enqueue("Recent completed work").unwrap();
        store.set_task(&task.id, "running", "").unwrap();
        store.set_task(&task.id, "completed", "").unwrap();
    }
    assert!(!store.tasks().unwrap().iter().any(|t| t.id == failures[0]));
    let attention = store.attention().unwrap();
    assert_eq!(attention.recent.len(), 6);
    assert!(
        attention
            .recent
            .iter()
            .all(|t| t.task.status == "completed")
    );
    assert_eq!(attention.outcome_count, 23);
    assert_eq!(attention.outcomes.len(), 20);
    assert_eq!(attention.outcomes[0].task.id, failures[0]);
    assert_eq!(attention.outcomes[0].request.chars().count(), 240);
    assert_eq!(
        store.task_messages(&failures[0]).unwrap()[0].content,
        "Original request ".repeat(50)
    );
    store.review_outcome(&failures[0]).unwrap();
    store.review_outcome(&failures[0]).unwrap();
    assert!(store.review_outcome("unknown").is_err());
    let queued = store.enqueue("Still active").unwrap();
    assert!(store.review_outcome(&queued.id).is_err());
    drop(store);
    let store = Store::open(&path).unwrap();
    let attention = store.attention().unwrap();
    assert_eq!(attention.outcome_count, 22);
    assert_eq!(attention.outcomes[0].task.id, failures[1]);
    assert_eq!(store.task(&failures[0]).unwrap().unwrap().status, "failed");
    assert_eq!(attention.active[0].task.id, queued.id);
}

#[test]
fn approvals_are_scoped_single_use_and_become_interrupted_attention_on_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.sqlite");
    let a = Store::open(&path).unwrap();
    let b = Store::open(&dir.path().join("b.sqlite")).unwrap();
    let task = a.enqueue("Read record A").unwrap();
    a.set_task(&task.id, "running", "Waiting").unwrap();
    let permission = a
        .add_permission(&task.id, "Read record", &json!({"record":"A"}))
        .unwrap();
    assert!(b.decide_permission(&permission, true).is_err());
    assert!(b.review_outcome(&task.id).is_err());
    assert_eq!(
        a.attention().unwrap().permissions[0].action,
        json!({"record":"A"})
    );
    a.decide_permission(&permission, false).unwrap();
    assert!(a.decide_permission(&permission, true).is_err());
    let pending = a
        .add_permission(&task.id, "Read again", &json!({}))
        .unwrap();
    drop(a);
    let a = Store::open(&path).unwrap();
    let attention = a.attention().unwrap();
    assert!(attention.active.is_empty());
    assert!(attention.permissions.is_empty());
    assert_eq!(attention.outcomes[0].task.status, "interrupted");
    assert!(a.decide_permission(&pending, true).is_err());
}

use ayati::storage::{Store, intake::NewSchedule};
use uuid::Uuid;

#[test]
fn schedule_history_is_bounded_scoped_and_preserves_outcomes_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("agent.sqlite");
    let store = Store::open(&path).unwrap();
    let other = Store::open(&dir.path().join("other.sqlite")).unwrap();
    let input = NewSchedule {
        id: Uuid::new_v4().to_string(),
        content: "Scheduled report".into(),
        run_at_ms: 1000,
        interval_seconds: Some(1),
    };
    store.add_schedule(&input).unwrap();
    assert!(store.schedule_runs(&input.id).unwrap().is_empty());
    assert!(other.schedule_runs(&input.id).is_err());
    for at in 1..=25 {
        store.dispatch_due_schedules(at * 1000).unwrap();
        let claim = store.claim_next("worker").unwrap().unwrap();
        store
            .finish_work(&claim, "completed", "Verified result")
            .unwrap();
    }
    let runs = store.schedule_runs(&input.id).unwrap();
    assert_eq!(runs.len(), 20);
    assert_eq!(runs[0].due_at_ms, 25000);
    assert_eq!(runs[19].due_at_ms, 6000);
    assert_eq!(runs[0].task.detail, "Verified result");
    store.dispatch_due_schedules(26000).unwrap();
    store.set_schedule_enabled(&input.id, false).unwrap();
    drop(store);
    let store = Store::open(&path).unwrap();
    assert_eq!(
        store.schedule_runs(&input.id).unwrap()[0].task.status,
        "stopped"
    );
    assert_eq!(store.schedules().unwrap()[0].state, "paused");
    // Even the same rule ID in a second private database has its own history.
    other.add_schedule(&input).unwrap();
    assert!(other.schedule_runs(&input.id).unwrap().is_empty());
    assert!(store.schedule_runs("missing").is_err());
}

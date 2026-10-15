use ayati::storage::{
    Store,
    intake::{LocalEvent, NewSchedule},
    work::IntakeError,
};
use rusqlite::Connection;
use std::sync::Arc;
use uuid::Uuid;

fn event(id: &str, content: &str) -> LocalEvent {
    LocalEvent {
        source: "fixture.shop".into(),
        event_id: id.into(),
        content: content.into(),
    }
}

fn schedule(at: i64, interval: Option<i64>) -> NewSchedule {
    NewSchedule {
        id: Uuid::new_v4().to_string(),
        content: "Make a synthetic stock summary".into(),
        run_at_ms: at,
        interval_seconds: interval,
    }
}

#[test]
fn schema_three_migration_preserves_accepted_queue_and_runtime_mapping() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(include_str!("../src/storage/schema.sql"))
        .unwrap();
    connection
        .execute_batch(include_str!("../src/storage/runtime.sql"))
        .unwrap();
    connection
        .execute_batch(include_str!("../src/storage/work.sql"))
        .unwrap();
    connection.execute_batch("INSERT INTO tasks(id,status,context) VALUES('accepted','queued','[]'); INSERT INTO messages(id,task_id,role,content,state) VALUES('message','accepted','user','Original work','complete'); INSERT INTO input_receipts VALUES('local-conversation','original-request','accepted','saved'); INSERT INTO run_scopes VALUES('accepted','local-conversation'); INSERT INTO runtime_sessions VALUES('local-conversation','saved-session','1.54.0','original');").unwrap();
    drop(connection);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.user_text("accepted").unwrap(), "Original work");
    assert_eq!(
        store.runtime_session("original").unwrap().as_deref(),
        Some("saved-session")
    );
    assert_eq!(
        store.claim_next("worker").unwrap().unwrap().task_id,
        "accepted"
    );
    assert!(!store.dispatch_paused().unwrap());
    assert!(store.schedules().unwrap().is_empty());
}

#[test]
fn events_deduplicate_by_source_and_id_and_reject_conflicting_retries() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    let store = Arc::new(Store::open(&path).unwrap());
    let ids: Vec<_> = std::thread::scope(|scope| {
        let threads: Vec<_> = (0..8)
            .map(|_| {
                scope.spawn(|| {
                    store
                        .accept_event(&event("order-1", "First event"))
                        .unwrap()
                        .id
                })
            })
            .collect();
        threads.into_iter().map(|t| t.join().unwrap()).collect()
    });
    assert!(ids.iter().all(|id| id == &ids[0]));
    assert_eq!(store.pending_count().unwrap(), 1);
    let conflict = store
        .accept_event(&event("order-1", "Different event"))
        .err()
        .unwrap();
    assert!(matches!(
        conflict.downcast_ref::<IntakeError>(),
        Some(IntakeError::Conflict)
    ));
    let another = LocalEvent {
        source: "another.shop".into(),
        ..event("order-1", "First event")
    };
    assert_ne!(store.accept_event(&another).unwrap().id, ids[0]);
    store.stop_work(&ids[0]).unwrap();
    drop(store);
    let store = Store::open(&path).unwrap();
    assert_eq!(
        store
            .accept_event(&event("order-1", "First event"))
            .unwrap()
            .status,
        "stopped"
    );
    for invalid in [
        event("", "ok"),
        event("new", ""),
        LocalEvent {
            source: "bad/source".into(),
            ..event("new", "ok")
        },
    ] {
        assert!(store.accept_event(&invalid).is_err());
    }
    assert_eq!(store.tasks().unwrap().len(), 2);
}

#[test]
fn scheduler_coalesces_downtime_and_never_overlaps_a_routine_or_replays_an_occurrence() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    let store = Arc::new(Store::open(&path).unwrap());
    let input = schedule(10000, Some(10));
    store.add_schedule(&input).unwrap();
    store.add_schedule(&input).unwrap();
    let mut changed = schedule(10000, Some(10));
    changed.id.clone_from(&input.id);
    changed.content = "Conflicting content".into();
    assert!(store.add_schedule(&changed).is_err());
    let deliveries: Vec<_> = std::thread::scope(|scope| {
        let threads: Vec<_> = (0..8)
            .map(|_| scope.spawn(|| store.dispatch_due_schedules(45001).unwrap()))
            .collect();
        threads
            .into_iter()
            .flat_map(|t| t.join().unwrap())
            .collect()
    });
    assert_eq!(deliveries.len(), 1);
    assert_eq!(store.schedules().unwrap()[0].next_run_ms, Some(50000));
    assert!(store.dispatch_due_schedules(95000).unwrap().is_empty());
    assert!(store.dispatch_due_schedules(20000).unwrap().is_empty()); // Clock moves backward.
    let first = store.claim_next("worker").unwrap().unwrap();
    store.finish_work(&first, "completed", "").unwrap();
    let second = store.dispatch_due_schedules(95000).unwrap();
    assert_eq!(second.len(), 1);
    assert_ne!(second[0], first.task_id);
    assert_eq!(store.schedules().unwrap()[0].next_run_ms, Some(100000));
    drop(store);
    let store = Store::open(&path).unwrap();
    assert!(store.dispatch_due_schedules(95000).unwrap().is_empty());
    let connection = Connection::open(&path).unwrap();
    let mut statement = connection
        .prepare("SELECT due_at_ms FROM schedule_runs ORDER BY due_at_ms")
        .unwrap();
    let due: Vec<i64> = statement
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(due, vec![40000, 90000]);
}

#[test]
fn full_queue_does_not_acknowledge_an_event_or_advance_a_due_schedule() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    let store = Store::open(&path).unwrap();
    let first = store.enqueue("First").unwrap();
    for _ in 1..32 {
        store.enqueue("More").unwrap();
    }
    let input = schedule(10000, Some(10));
    store.add_schedule(&input).unwrap();
    assert!(store.dispatch_due_schedules(95000).unwrap().is_empty());
    assert_eq!(store.schedules().unwrap()[0].next_run_ms, Some(10000));
    let rejected = store
        .accept_event(&event("order-full", "Event after capacity"))
        .err()
        .unwrap();
    assert!(matches!(
        rejected.downcast_ref::<IntakeError>(),
        Some(IntakeError::Full)
    ));
    let connection = Connection::open(&path).unwrap();
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM event_receipts", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    store.stop_work(&first.id).unwrap();
    let accepted = store
        .accept_event(&event("order-full", "Event after capacity"))
        .unwrap();
    assert_eq!(
        store
            .accept_event(&event("order-full", "Event after capacity"))
            .unwrap()
            .id,
        accepted.id
    );
    store.stop_work(&accepted.id).unwrap();
    assert_eq!(store.dispatch_due_schedules(95000).unwrap().len(), 1);
    assert_eq!(store.schedules().unwrap()[0].next_run_ms, Some(100000));
    assert_eq!(store.pending_count().unwrap(), 32);
}

#[test]
fn vm_stop_parks_future_intake_across_restart_and_terminal_chat_retry_cannot_resume_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    let store = Store::open(&path).unwrap();
    let request = Uuid::new_v4().to_string();
    let original = store.accept_input("Original", Some(&request)).unwrap();
    store.add_schedule(&schedule(10000, None)).unwrap();
    store.stop_all_work().unwrap();
    let waiting = store
        .accept_event(&event("while-off", "Event while off"))
        .unwrap();
    assert!(store.dispatch_due_schedules(20000).unwrap().is_empty());
    drop(store);
    let store = Store::open(&path).unwrap();
    assert!(store.dispatch_paused().unwrap());
    assert!(store.claim_next("worker").unwrap().is_none());
    assert_eq!(
        store
            .accept_input("Original", Some(&request))
            .unwrap()
            .status,
        "stopped"
    );
    assert!(store.dispatch_paused().unwrap());
    let deliberate = store.enqueue("New deliberate chat").unwrap();
    assert!(!store.dispatch_paused().unwrap());
    assert_eq!(store.dispatch_due_schedules(20000).unwrap().len(), 1);
    assert_eq!(store.task(&original.id).unwrap().unwrap().status, "stopped");
    assert_eq!(store.task(&waiting.id).unwrap().unwrap().status, "queued");
    assert_eq!(
        store.task(&deliberate.id).unwrap().unwrap().status,
        "queued"
    );
}

#[test]
fn schedule_pause_stops_queued_occurrences_and_resume_never_replays_them() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("state.sqlite")).unwrap();
    let input = schedule(10000, Some(10));
    store.add_schedule(&input).unwrap();
    let first = store.dispatch_due_schedules(15000).unwrap().remove(0);
    store.set_schedule_enabled(&input.id, false).unwrap();
    assert_eq!(store.task(&first).unwrap().unwrap().status, "stopped");
    assert!(store.dispatch_due_schedules(95000).unwrap().is_empty());
    store.set_schedule_enabled(&input.id, true).unwrap();
    let second = store.dispatch_due_schedules(95000).unwrap().remove(0);
    let claim = store.claim_next("worker").unwrap().unwrap();
    assert_eq!(claim.task_id, second);
    store.set_schedule_enabled(&input.id, false).unwrap();
    assert!(store.claim_active(&claim).unwrap());
    store.finish_work(&claim, "completed", "").unwrap();
    store.set_schedule_enabled(&input.id, true).unwrap();
    assert!(store.dispatch_due_schedules(95000).unwrap().is_empty());
    let once = schedule(1, None);
    store.add_schedule(&once).unwrap();
    assert_eq!(store.dispatch_due_schedules(95000).unwrap().len(), 1);
    assert!(store.set_schedule_enabled(&once.id, true).is_err());
}

#[test]
fn database_failure_rolls_back_event_and_schedule_acceptance_before_acknowledgement() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    let store = Store::open(&path).unwrap();
    let input = schedule(10000, None);
    store.add_schedule(&input).unwrap();
    let connection = Connection::open(&path).unwrap();
    connection.execute_batch("CREATE TRIGGER fail_event BEFORE INSERT ON event_receipts BEGIN SELECT RAISE(ABORT,'fixture receipt failure'); END; CREATE TRIGGER fail_schedule BEFORE UPDATE ON schedules BEGIN SELECT RAISE(ABORT,'fixture cursor failure'); END;").unwrap();
    assert!(
        store
            .accept_event(&event("atomic", "Synthetic event"))
            .is_err()
    );
    assert!(store.dispatch_due_schedules(20000).is_err());
    assert!(store.tasks().unwrap().is_empty());
    assert!(store.messages().unwrap().is_empty());
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM input_receipts", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM schedule_runs", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(store.schedules().unwrap()[0].next_run_ms, Some(10000));
    connection
        .execute_batch("DROP TRIGGER fail_event; DROP TRIGGER fail_schedule;")
        .unwrap();
    assert_eq!(
        store
            .accept_event(&event("atomic", "Synthetic event"))
            .unwrap()
            .status,
        "queued"
    );
    assert_eq!(store.dispatch_due_schedules(20000).unwrap().len(), 1);
    assert!(store.dispatch_due_schedules(20000).unwrap().is_empty());
}

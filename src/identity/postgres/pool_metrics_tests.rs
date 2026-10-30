//! Pool metrics count real queued work, slots and admission waits.
use super::{executor_slot_tests::reserve, executor_task_tests::executor, tests::*, *};

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn pool_snapshot_counts_waiting_work_slots_and_admission_waits() {
    let (db, owner, w, chat, agent) = super::rehearsal_tests::setup().await;
    let before = db.pool_snapshot().await.unwrap();
    db.start_rehearsal(&owner, &w, &agent, &super::rehearsal_tests::start())
        .await
        .unwrap();
    db.chat_send(
        &owner,
        &w,
        &chat,
        &message("Waiting question", Some(&agent)),
    )
    .await
    .unwrap();
    execute(&db.db,"UPDATE relay_chat_jobs SET dispatch='pooled',created_ms=created_ms-5000 WHERE workspace=$1",vec![w.clone().into()]).await.unwrap();
    let (registration, _) = executor(&db).await;
    let waiting = db.pool_snapshot().await.unwrap();
    assert_eq!(waiting.queued, before.queued + 2);
    assert!(waiting.oldest_queued_seconds >= 5);
    assert!(waiting.ready_slots >= 1);
    assert!(waiting.database_connections >= 1 && waiting.database_bytes > 0);
    // Reserving takes the admission lock; the snapshot reports it once.
    let scope = reserve(&db, &registration).await.unwrap();
    assert!(!scope.chat.is_empty(), "chat is offered first");
    let busy = db.pool_snapshot().await.unwrap();
    assert_eq!(busy.queued, waiting.queued - 1);
    assert!(busy.busy_slots >= 1);
    // The wait counter is process-wide; parallel tests may add to it.
    assert!(busy.admission_waits >= 1 && busy.admission_wait_max_ms >= 0.0);
    let tables = db.largest_tables(5).await.unwrap();
    assert!(!tables.is_empty() && tables.len() <= 5 && tables[0].1 > 0);
}

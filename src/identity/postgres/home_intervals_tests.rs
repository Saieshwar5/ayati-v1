//! A panel's fixed interval runs on its own update queue (schema 146): no task, the
//! next update booked one interval after each update starts, and no gap holds it
//! back.
use super::super::super::activity::Filters;
use super::super::super::home_maintenance_tests::{
    agreement, claim, due_now, observed, panel, refresh, setup,
};
use super::super::super::tests::message;
use super::super::super::*;

async fn wanted(db: &PgStore, panel: &str) -> Value {
    query(
        &db.db,
        "SELECT to_jsonb(k) AS v FROM relay_home_update_wanted k WHERE panel=$1",
        vec![panel.into()],
    )
    .await
    .unwrap()
    .map(|r| r.try_get("", "v").unwrap())
    .unwrap_or(Value::Null)
}

async fn runs(db: &PgStore, panel: &str) -> Vec<Value> {
    rows(
        &db.db,
        "SELECT to_jsonb(r) AS v FROM relay_home_update_runs r WHERE panel=$1 ORDER BY created_ms",
        vec![panel.into()],
    )
    .await
    .unwrap()
    .into_iter()
    .map(|r| r.try_get("", "v").unwrap())
    .collect()
}

async fn count(db: &PgStore, sql: &str) -> i64 {
    query(&db.db, sql, vec![])
        .await
        .unwrap()
        .unwrap()
        .try_get("", "n")
        .unwrap()
}

async fn now(db: &PgStore) -> i64 {
    count(
        db,
        "SELECT (extract(epoch FROM clock_timestamp())*1000)::bigint AS n",
    )
    .await
}

/// Finish an update with a fresh snapshot, as the agent would.
async fn finish(db: &PgStore, w: &str, reply: &str, read: &str, value: &str) {
    let job = claim(db, w, reply).await;
    observed(db, &job, read).await;
    db.refresh_home_panel(&job, &refresh(value)).await.unwrap();
    db.chat_finish(&job, "Updated", "complete").await.unwrap();
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn an_interval_update_runs_on_the_panels_queue_and_books_the_next_one() {
    let (db, owner, w, first, read, _, p) = setup().await;
    let tasks = count(&db, "SELECT count(*) AS n FROM relay_tasks").await;
    let before = now(&db).await;
    db.save_home_agreement(&owner, &w, &p.id, &agreement(0, Some(120)))
        .await
        .unwrap();
    let after = now(&db).await;
    let want = wanted(&db, &p.id).await;
    assert_eq!(want["reason"], "interval");
    let due = want["due_ms"].as_i64().unwrap();
    assert!((before + 120_000..=after + 120_000).contains(&due));
    assert_eq!(panel(&db, &owner, &w).await["maintenance"]["next_ms"], due);
    assert_eq!(db.dispatch_panel_updates().await.unwrap(), 0, "not due yet");

    // When it is due it starts as the owner and books the next one an interval later.
    due_now(&db, &p.id).await;
    assert_eq!(db.dispatch_panel_updates().await.unwrap(), 1);
    let started = runs(&db, &p.id).await;
    assert_eq!(started.len(), 1);
    assert_eq!(started[0]["reason"], "interval");
    let created = started[0]["created_ms"].as_i64().unwrap();
    let next = wanted(&db, &p.id).await;
    assert_eq!(next["reason"], "interval");
    let next_due = next["due_ms"].as_i64().unwrap();
    assert!((created + 120_000..created + 130_000).contains(&next_due));
    assert_eq!(
        panel(&db, &owner, &w).await["maintenance"]["next_ms"],
        next_due
    );
    let page = db
        .activity_history(
            &owner,
            &w,
            &Filters {
                kind: Some("panel".into()),
                ..Filters::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(page["items"][0]["summary"], "Updated panel: Personal order");
    assert_eq!(
        count(&db, "SELECT count(*) AS n FROM relay_tasks").await,
        tasks,
        "a panel update is not a task"
    );

    // Due while that update still runs, it waits for it.
    due_now(&db, &p.id).await;
    assert_eq!(db.dispatch_panel_updates().await.unwrap(), 0);
    assert!(
        wanted(&db, &p.id).await["detail"]
            .as_str()
            .unwrap()
            .contains("current update")
    );
    finish(&db, &w, started[0]["reply"].as_str().unwrap(), &read, "2").await;
    let state = panel(&db, &owner, &w).await;
    assert_eq!(state["maintenance"]["status"], "current");

    // No gap holds the fixed interval back (the default gap is five minutes).
    due_now(&db, &p.id).await;
    assert_eq!(db.dispatch_panel_updates().await.unwrap(), 1);
    let started = runs(&db, &p.id).await;
    assert_eq!(started.len(), 2);
    finish(&db, &w, started[1]["reply"].as_str().unwrap(), &read, "3").await;

    // Work on its sources within the gap waits only until the next interval update.
    let sent = db
        .chat_send(
            &owner,
            &w,
            &first.chat,
            &message("How many orders?", Some(&first.agent)),
        )
        .await
        .unwrap()
        .unwrap();
    let work = claim(&db, &w, &sent.reply).await;
    observed(&db, &work, &read).await;
    db.chat_finish(&work, "Two", "complete").await.unwrap();
    assert_eq!(wanted(&db, &p.id).await["reason"], "work");
    assert_eq!(
        db.dispatch_panel_updates().await.unwrap(),
        0,
        "within the gap"
    );
    let held = wanted(&db, &p.id).await;
    assert_eq!(held["reason"], "interval");
    assert_eq!(
        held["due_ms"].as_i64().unwrap(),
        started[1]["created_ms"].as_i64().unwrap() + 120_000
    );

    // A changed interval starts again from now; pausing drops it.
    let before = now(&db).await;
    db.save_home_agreement(&owner, &w, &p.id, &agreement(1, Some(600)))
        .await
        .unwrap();
    assert!(wanted(&db, &p.id).await["due_ms"].as_i64().unwrap() >= before + 600_000);
    let mut paused = agreement(2, Some(600));
    paused.enabled = false;
    db.save_home_agreement(&owner, &w, &p.id, &paused)
        .await
        .unwrap();
    assert!(wanted(&db, &p.id).await.is_null());
    assert!(panel(&db, &owner, &w).await["maintenance"]["next_ms"].is_null());
}

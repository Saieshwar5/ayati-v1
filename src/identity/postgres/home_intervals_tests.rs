//! A panel's fixed interval runs on its own update queue (schema 146): no schedule,
//! no task, the next update booked one interval after each update starts, and no
//! gap holds it back. Panels that used a schedule keep their next update time.
use super::super::super::activity::Filters;
use super::super::super::home_maintenance_tests::{
    agreement, claim, due_now, observed, panel, refresh, setup, setup_on,
};
use super::super::super::tests::message;
use super::super::super::*;
use sea_orm_migration::MigratorTrait;
use serde_json::json;

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
    assert_eq!(
        count(&db, "SELECT count(*) AS n FROM relay_agent_schedules").await,
        0,
        "no schedule"
    );
    assert_eq!(
        count(
            &db,
            "SELECT count(*) AS n FROM relay_home_schedule_bindings"
        )
        .await,
        0
    );
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

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn schema_146_moves_a_scheduled_panel_onto_its_queue_and_its_schedule_stops() {
    let url = std::env::var("RELAY_TEST_DATABASE_URL").unwrap();
    let admin = PgStore::connect(&url).await.unwrap();
    let schema = format!("relay_panel_intervals_{}", uuid::Uuid::new_v4().simple());
    admin
        .db
        .execute_unprepared(&format!("CREATE SCHEMA {schema}"))
        .await
        .unwrap();
    let mut scoped = reqwest::Url::parse(&url).unwrap();
    scoped
        .query_pairs_mut()
        .append_pair("options", &format!("-c search_path={schema}"));
    let db = PgStore::connect(scoped.as_str()).await.unwrap();
    super::super::super::migration::Migrator::up(&db.db, Some(145))
        .await
        .unwrap();
    let (db, owner, w, first, read, _, p) = setup_on(db).await;
    // Current code on schema 145 books nothing on the queue.
    db.save_home_agreement(&owner, &w, &p.id, &agreement(0, Some(60)))
        .await
        .unwrap();
    assert!(wanted(&db, &p.id).await.is_null());

    // The panel's interval as the schema 145 controller saved it: a bound schedule.
    let sid = uuid::Uuid::new_v4().to_string();
    let first_ms = now(&db).await + 60_000;
    execute(&db.db, "INSERT INTO relay_agent_schedules(workspace,id,agent,principal,chat,name,content,first_ms,interval_seconds,timezone,request_hash,next_ms) VALUES($1,$2,$3,$4,$5,'Personal Home panel update','Refresh the controller-bound personal Home panel.',$6,60,'UTC',$2,$6)",
        vec![w.clone().into(), sid.clone().into(), first.agent.clone().into(), owner.id.clone().into(), first.chat.clone().into(), first_ms.into()]).await.unwrap();
    execute(
        &db.db,
        "INSERT INTO relay_home_schedule_bindings VALUES($1,$2,$3)",
        vec![w.clone().into(), sid.clone().into(), p.id.clone().into()],
    )
    .await
    .unwrap();
    execute(
        &db.db,
        "UPDATE relay_home_maintenance SET schedule=$3 WHERE workspace=$1 AND panel=$2",
        vec![w.clone().into(), p.id.clone().into(), sid.clone().into()],
    )
    .await
    .unwrap();
    // One scheduled update ran before the upgrade.
    assert_eq!(db.dispatch_schedules_at(first_ms).await.unwrap(), 1);
    let old = runs(&db, &p.id).await;
    assert_eq!(old.len(), 1);
    finish(&db, &w, old[0]["reply"].as_str().unwrap(), &read, "7").await;
    let next_ms = first_ms + 60_000;
    assert_eq!(
        panel(&db, &owner, &w).await["maintenance"]["next_ms"],
        next_ms
    );

    assert!(db.verify_schema().await.is_err());
    db.migrate().await.unwrap();
    db.migrate().await.unwrap();
    db.verify_schema().await.unwrap();

    // Same next update, now on the panel's own queue; the schedule never runs again.
    let want = wanted(&db, &p.id).await;
    assert_eq!(
        (want["reason"].as_str(), want["due_ms"].as_i64()),
        (Some("interval"), Some(next_ms))
    );
    let state = panel(&db, &owner, &w).await;
    assert_eq!(state["maintenance"]["next_ms"], next_ms);
    assert!(state["maintenance"]["schedule"].is_null());
    assert_eq!(state["maintenance"]["status"], "current");
    assert_eq!(
        state["active"]["definition"]["sections"][0]["value"], "7",
        "the earlier scheduled update stays readable"
    );
    let schedule = query(
        &db.db,
        "SELECT state FROM relay_agent_schedules WHERE id=$1",
        vec![sid.clone().into()],
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(schedule.try_get::<String>("", "state").unwrap(), "paused");
    assert_eq!(
        db.dispatch_schedules_at(next_ms + 600_000).await.unwrap(),
        0
    );
    assert_eq!(runs(&db, &p.id).await.len(), 1);
    assert_eq!(
        db.schedules(&owner, &w, &first.agent).await.unwrap()["schedules"],
        json!([]),
        "panel schedules stay out of the agent's list"
    );

    // When due, the panel's queue runs it and books the next one.
    due_now(&db, &p.id).await;
    assert_eq!(db.dispatch_panel_updates().await.unwrap(), 1);
    let started = runs(&db, &p.id).await;
    assert_eq!(started.len(), 2);
    assert_eq!(started[1]["reason"], "interval");
    assert_eq!(wanted(&db, &p.id).await["reason"], "interval");
}

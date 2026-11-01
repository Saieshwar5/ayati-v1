use super::{tests::*, *};
use crate::identity::automations::model::{Change, Schedule};
use serde_json::json;
pub(super) async fn setup() -> (PgStore, User, String, String, String) {
    let mut db = fixture().await;
    db.pooled = true;
    let user = user(&db, "schedule-author").await;
    let w = workspace(&db, &user).await;
    let (chat, agent) = super::executor_slot_tests::agent(&db, &user, &w).await;
    (db, user, w, chat, agent)
}
pub(super) fn schedule(chat: &str, repeat: Option<i64>) -> Schedule {
    Schedule {
        id: uuid::Uuid::new_v4().to_string(),
        chat: Some(chat.into()),
        name: "Delivery review".into(),
        content: "Review delivery exceptions. Ask separately before changing an order.".into(),
        first_ms: now() * 1000 + 60000,
        interval_seconds: repeat,
        timezone: "Asia/Kolkata".into(),
    }
}
pub(super) async fn state(db: &PgStore, w: &str, id: &str) -> Value {
    query(
        &db.db,
        "SELECT to_jsonb(s) AS data FROM relay_agent_schedules s WHERE workspace=$1 AND id=$2",
        vec![w.into(), id.into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "data")
    .unwrap()
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn automation_atomic_occurrence_and_exact_creation_survive_replica_duplicates_and_unavailable_mode()
 {
    let (mut db, owner, w, c, a) = setup().await;
    let input = schedule(&c, None);
    let (one, two) = tokio::join!(
        db.save_schedule(&owner, &w, &a, &input),
        db.save_schedule(&owner, &w, &a, &input)
    );
    assert_eq!(one.unwrap(), two.unwrap());
    let mut changed = input.clone();
    changed.content = "Different task".into();
    assert!(db.save_schedule(&owner, &w, &a, &changed).await.is_err());
    let at = input.first_ms;
    let (one, two) = tokio::join!(db.dispatch_schedules_at(at), db.dispatch_schedules_at(at));
    assert_eq!(one.unwrap() + two.unwrap(), 1);
    assert_eq!(state(&db, &w, &input.id).await["state"], "completed");
    assert_eq!(db.dispatch_schedules_at(at + 100000).await.unwrap(), 0);
    let runs = db.schedule_runs(&owner, &w, &a, &input.id).await.unwrap();
    assert_eq!(runs["runs"].as_array().unwrap().len(), 1);
    let job: Value = query(
        &db.db,
        "SELECT to_jsonb(j) AS data FROM relay_chat_jobs j",
        vec![],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "data")
    .unwrap();
    assert_eq!(job["work_class"], "background");
    assert_eq!(job["dispatch"], "pooled");
    assert_eq!(job["payload"]["requester"], owner.id);
    // The run is in the schedule's own work room, not the conversation it posts to.
    assert_eq!(job["payload"]["chat"], runs["runs"][0]["chat"]);
    assert_ne!(job["payload"]["chat"], c);
    assert_eq!(job["request_message"], runs["runs"][0]["message"]);
    assert!(
        job["payload"]["prompt"]
            .as_str()
            .unwrap()
            .contains("Scheduled work · Delivery review")
    );
    db.pooled = false;
    assert_eq!(
        db.save_schedule(&owner, &w, &a, &input).await.unwrap()["state"],
        "completed"
    );
    assert!(
        db.save_schedule(&owner, &w, &a, &schedule(&c, None))
            .await
            .is_err()
    );
    assert_eq!(db.dispatch_schedules_at(at + 200000).await.unwrap(), 0);
    assert!(
        db.change_schedule(
            &owner,
            &w,
            &a,
            &input.id,
            &Change {
                request: uuid::Uuid::new_v4().to_string(),
                revision: 2,
                enabled: true
            }
        )
        .await
        .is_err()
    );
    assert_eq!(db.company_counts(&owner, &w).await.unwrap()["jobs"], 1);
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn automation_coalesces_missed_intervals_and_never_overlaps_or_replays_a_stopped_occurrence()
{
    let (db, owner, w, c, a) = setup().await;
    let input = schedule(&c, Some(10));
    db.save_schedule(&owner, &w, &a, &input).await.unwrap();
    assert_eq!(
        db.dispatch_schedules_at(input.first_ms + 105000)
            .await
            .unwrap(),
        1
    );
    let runs = db.schedule_runs(&owner, &w, &a, &input.id).await.unwrap();
    assert_eq!(runs["runs"][0]["due_ms"], input.first_ms + 100000);
    assert_eq!(
        state(&db, &w, &input.id).await["next_ms"],
        input.first_ms + 110000
    );
    assert_eq!(
        db.dispatch_schedules_at(input.first_ms + 305000)
            .await
            .unwrap(),
        0
    );
    let reply = runs["runs"][0]["reply"].as_str().unwrap();
    let room = runs["runs"][0]["chat"].as_str().unwrap();
    db.cancel_assignment(&owner, &w, room, reply).await.unwrap();
    assert_eq!(
        db.dispatch_schedules_at(input.first_ms + 305000)
            .await
            .unwrap(),
        1
    );
    let runs = db.schedule_runs(&owner, &w, &a, &input.id).await.unwrap();
    assert_eq!(runs["runs"].as_array().unwrap().len(), 2);
    assert_eq!(runs["runs"][0]["due_ms"], input.first_ms + 300000);
    assert_eq!(runs["runs"][1]["state"], "cancelled");
    assert_eq!(
        db.dispatch_schedules_at(input.first_ms + 100000)
            .await
            .unwrap(),
        0
    );
    assert_eq!(state(&db, &w, &input.id).await["state"], "enabled");
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn automation_queue_pressure_cannot_partially_accept_an_occurrence_or_advance_its_cursor() {
    let (db, owner, w, c, a) = setup().await;
    for _ in 0..32 {
        db.chat_background(
            &owner,
            &w,
            &c,
            &message("Existing background work", Some(&a)),
        )
        .await
        .unwrap();
    }
    let input = schedule(&c, Some(10));
    db.save_schedule(&owner, &w, &a, &input).await.unwrap();
    assert_eq!(db.dispatch_schedules_at(input.first_ms).await.unwrap(), 0);
    assert_eq!(state(&db, &w, &input.id).await["next_ms"], input.first_ms);
    assert!(
        state(&db, &w, &input.id).await["detail"]
            .as_str()
            .unwrap()
            .contains("queue capacity")
    );
    assert_eq!(
        db.schedule_runs(&owner, &w, &a, &input.id).await.unwrap()["runs"],
        json!([])
    );
    assert_eq!(db.company_counts(&owner, &w).await.unwrap()["jobs"], 32);
    let reply: String = query(
        &db.db,
        "SELECT reply FROM relay_chat_jobs ORDER BY sequence LIMIT 1",
        vec![],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "reply")
    .unwrap();
    db.cancel_assignment(&owner, &w, &c, &reply).await.unwrap();
    assert_eq!(db.dispatch_schedules_at(input.first_ms).await.unwrap(), 1);
    assert_eq!(db.company_counts(&owner, &w).await.unwrap()["jobs"], 33);
    assert_eq!(
        db.schedule_runs(&owner, &w, &a, &input.id).await.unwrap()["runs"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn automation_pause_receipts_are_exact_and_only_cancel_unstarted_initial_work() {
    let (db, owner, w, c, a) = setup().await;
    let input = schedule(&c, Some(10));
    db.save_schedule(&owner, &w, &a, &input).await.unwrap();
    db.dispatch_schedules_at(input.first_ms).await.unwrap();
    let pause = Change {
        request: uuid::Uuid::new_v4().to_string(),
        revision: 1,
        enabled: false,
    };
    let saved = db
        .change_schedule(&owner, &w, &a, &input.id, &pause)
        .await
        .unwrap();
    assert_eq!(saved["state"], "paused");
    assert_eq!(
        db.schedule_runs(&owner, &w, &a, &input.id).await.unwrap()["runs"][0]["state"],
        "cancelled"
    );
    assert_eq!(
        db.dispatch_schedules_at(input.first_ms + 50000)
            .await
            .unwrap(),
        0
    );
    let resume = Change {
        request: uuid::Uuid::new_v4().to_string(),
        revision: 2,
        enabled: true,
    };
    db.change_schedule(&owner, &w, &a, &input.id, &resume)
        .await
        .unwrap();
    assert_eq!(
        db.change_schedule(&owner, &w, &a, &input.id, &pause)
            .await
            .unwrap(),
        saved
    );
    assert_eq!(state(&db, &w, &input.id).await["state"], "enabled");
    assert!(
        db.change_schedule(
            &owner,
            &w,
            &a,
            &input.id,
            &Change {
                enabled: true,
                ..pause
            }
        )
        .await
        .is_err()
    );
    assert_eq!(
        db.dispatch_schedules_at(input.first_ms + 50000)
            .await
            .unwrap(),
        1
    );
    // A saved question/approval continuation already ran. Routine pause must not
    // erase it; only its independent task Stop has that authority.
    execute(
        &db.db,
        "UPDATE relay_chat_jobs SET run_cycle=1 WHERE state='queued'",
        vec![],
    )
    .await
    .unwrap();
    db.change_schedule(
        &owner,
        &w,
        &a,
        &input.id,
        &Change {
            request: uuid::Uuid::new_v4().to_string(),
            revision: 3,
            enabled: false,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        db.schedule_runs(&owner, &w, &a, &input.id).await.unwrap()["runs"][0]["state"],
        "queued"
    );
}

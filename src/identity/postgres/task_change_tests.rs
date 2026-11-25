//! Tasks changed, paused and skipped from chat and Activity, bounded, and long jobs
//! continued part by part (schema 144).
use super::super::control_tests::setup;
use super::super::*;
use super::lifetime_tests::{at, claim, due_now, last_run, one, pending, task, wakeups};
use serde_json::json;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn tasks_change_pause_and_skip_from_chat_and_are_bounded() {
    let (mut db, owner, w, _, job) = setup("allow").await;
    db.pooled = true;
    let id = db.create_task(&job, &task(json!({"title":"Chase invoice 318","repeat":{"unit":"day","every":3,"at":"10:00"},"first_at":"2030-01-07 10:00"}))).await.unwrap()["id"].as_str().unwrap().to_string();
    let first = pending(&db, &id).await;
    assert_eq!(
        first[0]["due_ms"].as_i64().unwrap(),
        at(&db, "2030-01-07 10:00", "Asia/Kolkata").await,
        "first_at is local time"
    );
    let change =
        |value: Value| serde_json::from_value::<super::changes::ChangeTask>(value).unwrap();
    let changed = db
        .change_task(
            &job,
            &change(json!({"task": id, "repeat":{"unit":"day","every":2,"at":"16:00"}})),
        )
        .await
        .unwrap();
    assert_eq!(changed["task"]["rule_text"], "Every 2 days at 16:00");
    assert_eq!(pending(&db, &id).await.len(), 1, "rebooked, not doubled");
    db.change_task(&job, &change(json!({"task": id, "paused": true})))
        .await
        .unwrap();
    let paused = one(
        &db,
        "SELECT paused_ms IS NOT NULL AS paused,paused_reason FROM relay_tasks WHERE id=$1",
        vec![id.clone().into()],
    )
    .await;
    assert_eq!(paused["paused"], true);
    assert_eq!(paused["paused_reason"], "Paused in chat.");
    assert!(pending(&db, &id).await.is_empty());
    db.change_task(&job, &change(json!({"task": id, "paused": false})))
        .await
        .unwrap();
    let before = pending(&db, &id).await[0]["due_ms"].as_i64().unwrap();
    let skipped = db.task_skip_next(&owner, &w, &id).await.unwrap();
    assert!(
        skipped["next_ms"].as_i64().unwrap() > before,
        "the run after it is booked"
    );
    assert!(
        wakeups(&db, &id)
            .await
            .iter()
            .any(|k| k["state"] == "skipped" && k["due_ms"].as_i64() == Some(before))
    );
    // Wrong dates are refused without breaking the request.
    for bad in [
        json!({"task": id, "first_at": "2030-02-30 10:00"}),
        json!({"task": id, "until": "2020-01-01"}),
        json!({"task": id, "first_at": "tomorrow"}),
    ] {
        assert!(db.change_task(&job, &change(bad)).await.is_err());
    }
    assert!(
        db.create_task(
            &job,
            &task(json!({"title":"x","repeat":{"unit":"day","every":0}}))
        )
        .await
        .is_err()
    );
    // A run that started on its own may only make a repeating task that ends.
    let ask = super::super::tests::message("New orders arrived", Some(&job.agent));
    let queued = db
        .chat_background(&owner, &w, &job.chat, &ask)
        .await
        .unwrap()
        .unwrap();
    let background = claim(&db, &queued.reply).await;
    let endless = db
        .create_task(
            &background,
            &task(json!({"title":"Check every hour forever","repeat":{"unit":"hour"}})),
        )
        .await;
    assert!(
        endless
            .unwrap_err()
            .to_string()
            .contains("only create a repeating task that ends")
    );
    db.create_task(&background, &task(json!({"title":"Check every hour this week","repeat":{"unit":"hour"},"until":"2030-01-01"}))).await.unwrap();
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_long_job_continues_part_by_part_and_asks_after_twenty_parts() {
    let (mut db, _, _, _, job) = setup("allow").await;
    db.pooled = true;
    let id = db
        .create_task(&job, &task(json!({"title":"Prepare the quarterly report"})))
        .await
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let later = |done: &str| {
        serde_json::from_value::<super::changes::ContinueLater>(
            json!({"task": id, "done": done, "next": "Read the March sheets"}),
        )
        .unwrap()
    };
    let first = db
        .continue_later(&job, &later("Read the January and February sheets"))
        .await
        .unwrap();
    assert_eq!(first["state"], "continuing");
    db.chat_finish(&job, "Read two months so far.", "complete")
        .await
        .unwrap();
    let booked = pending(&db, &id).await;
    assert_eq!(booked[0]["kind"], "continue");
    due_now(&db, &id).await;
    assert_eq!(db.dispatch_task_wakeups().await.unwrap(), 1);
    let (reply, content) = last_run(&db, &id).await;
    assert!(
        content.starts_with("Continue · Prepare the quarterly report"),
        "{content}"
    );
    assert!(content.contains("Read the March sheets") && content.contains("January and February"));
    let part = claim(&db, &reply).await;
    for _ in 0..19 {
        assert_eq!(
            db.continue_later(&part, &later("More")).await.unwrap()["state"],
            "continuing"
        );
    }
    assert_eq!(
        db.continue_later(&part, &later("More")).await.unwrap()["state"],
        "refused",
        "ask the person after 20 parts"
    );
}

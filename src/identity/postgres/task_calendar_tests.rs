//! Activity › Calendar, the workspace calendar in Settings and the task line in Chat
//! (designs/agent-tasks).
use super::super::control_tests::setup;
use super::super::tests::{join, user};
use super::super::*;
use serde_json::json;

async fn one(db: &PgStore, sql: &str, args: Vec<sea_orm::Value>) -> Value {
    super::super::tasks::lifetime_tests::one(db, sql, args).await
}

async fn at(db: &PgStore, local: &str, zone: &str) -> i64 {
    super::super::tasks::lifetime_tests::at(db, local, zone).await
}

async fn next(
    db: &PgStore,
    w: &str,
    rule: Value,
    zone: &str,
    anchor: &str,
    after: &str,
) -> Option<i64> {
    let (anchor, after) = (at(db, anchor, zone).await, at(db, after, zone).await);
    one(
        db,
        "SELECT relay_task_next_due($1::jsonb,$2,$3,$4,$5) AS due",
        vec![
            rule.into(),
            zone.into(),
            anchor.into(),
            after.into(),
            w.into(),
        ],
    )
    .await["due"]
        .as_i64()
}

async fn due_now(db: &PgStore, task: &str) {
    execute(&db.db, "UPDATE relay_task_wakeups SET due_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint-1000 WHERE task=$1 AND state='pending'", vec![task.into()]).await.unwrap();
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn the_calendar_shows_past_and_planned_runs_to_the_right_people() {
    let (mut db, owner, w, _, job) = setup("allow").await;
    db.pooled = true;
    let made = db.create_task(&job, &serde_json::from_value(json!({"title":"Check stock below reorder level","repeat":{"unit":"day","at":"10:00"}})).unwrap()).await.unwrap();
    let id = made["id"].as_str().unwrap().to_string();
    db.chat_finish(
        &job,
        "Done. I'll check stock every day at 10:00.",
        "complete",
    )
    .await
    .unwrap();
    // The line under the reply names the next run.
    let page = db.chat_page(&owner, &w, &job.chat, None).await.unwrap();
    let reply = page["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == job.reply.as_str())
        .unwrap()
        .clone();
    assert_eq!(reply["task"]["repeats"], true);
    assert_eq!(reply["task"]["requester"], owner.id.as_str());
    assert!(
        reply["task"]["next_text"]
            .as_str()
            .unwrap()
            .ends_with("10:00")
    );
    due_now(&db, &id).await;
    assert_eq!(db.dispatch_task_wakeups().await.unwrap(), 1);

    let now = super::tasks::runs::now_ms(&db.db).await.unwrap();
    let week = db
        .activity_calendar(
            &owner,
            &w,
            now - 86_400_000,
            now + 7 * 86_400_000,
            Some("all"),
        )
        .await
        .unwrap();
    assert_eq!(week["timezone"], "Asia/Kolkata");
    let items = week["items"].as_array().unwrap();
    assert!(
        items
            .iter()
            .any(|i| i["kind"] == "run" && i["task"] == id.as_str() && i["state"] == "working"),
        "{items:?}"
    );
    let planned = items
        .iter()
        .filter(|i| i["kind"] == "planned" && i["task"] == id.as_str())
        .count();
    assert!(
        (6..=8).contains(&planned),
        "a daily rule draws each day of the week: {planned}"
    );
    assert!(
        db.activity_calendar(&owner, &w, now, now + 50 * 86_400_000, None)
            .await
            .is_err(),
        "six weeks at most"
    );

    db.task_set_paused(&owner, &w, &id, true).await.unwrap();
    let paused = db
        .activity_calendar(&owner, &w, now, now + 86_400_000, None)
        .await
        .unwrap();
    assert!(
        paused["untimed"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["kind"] == "paused" && i["task"] == id.as_str())
    );
    assert!(
        !paused["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["kind"] == "planned"),
        "nothing planned while paused"
    );

    // A member who neither asked nor looks after the agent doesn't see it.
    let member = user(&db, "calendar-member").await;
    join(&db, &owner, &w, &member, "Member").await;
    let theirs = db
        .activity_calendar(&member, &w, now - 86_400_000, now + 86_400_000, Some("all"))
        .await
        .unwrap();
    assert!(
        theirs["items"].as_array().unwrap().is_empty()
            && theirs["untimed"]
                .as_array()
                .unwrap()
                .iter()
                .all(|i| i["task"] != id.as_str())
    );
    assert!(
        db.task_set_paused(&member, &w, &id, false).await.is_err(),
        "only its person, owners and admins"
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn admins_keep_working_days_and_holidays_and_runs_follow_them() {
    let (db, owner, w, _, _) = setup("allow").await;
    let calendar = db.workspace_calendar(&owner, &w).await.unwrap();
    assert_eq!(calendar["working_days"], json!([1, 2, 3, 4, 5, 6]));
    assert_eq!(calendar["holiday_rule"], "next");
    assert_eq!(calendar["editable"], true);
    let input = |value: serde_json::Value| {
        serde_json::from_value::<super::WorkspaceCalendar>(value).unwrap()
    };
    let saved = db.save_workspace_calendar(&owner, &w, &input(json!({"working_days":[1,2,3,4,5],"holiday_rule":"skip",
        "holidays":[{"day":"2026-12-25","name":"Christmas"},{"day":"2027-01-26","name":"Republic Day"}]}))).await.unwrap();
    assert_eq!(saved["working_days"], json!([1, 2, 3, 4, 5]));
    assert_eq!(
        saved["holidays"][0],
        json!({"day":"2026-12-25","name":"Christmas"})
    );
    for bad in [
        json!({"working_days":[],"holiday_rule":"next","holidays":[]}),
        json!({"working_days":[1,1],"holiday_rule":"next","holidays":[]}),
        json!({"working_days":[1],"holiday_rule":"later","holidays":[]}),
        json!({"working_days":[1],"holiday_rule":"next","holidays":[{"day":"2026-02-30","name":"x"}]}),
    ] {
        assert!(
            db.save_workspace_calendar(&owner, &w, &input(bad))
                .await
                .is_err()
        );
    }
    let member = user(&db, "calendar-member").await;
    join(&db, &owner, &w, &member, "Member").await;
    assert_eq!(
        db.workspace_calendar(&member, &w).await.unwrap()["editable"],
        false,
        "members read it"
    );
    assert!(
        db.save_workspace_calendar(
            &member,
            &w,
            &input(json!({"working_days":[1],"holiday_rule":"next","holidays":[]}))
        )
        .await
        .is_err()
    );
    // Christmas 2026 is a Friday: a working-days rule skips it and the weekend.
    let rule: super::tasks::rules::Rule =
        serde_json::from_value(json!({"unit":"day","at":"10:00","working_days":true})).unwrap();
    let at = |local: &str| {
        let local = local.to_string();
        let db = &db;
        async move {
            query(&db.db, "SELECT (extract(epoch FROM ($1::timestamp AT TIME ZONE 'Asia/Kolkata'))*1000)::bigint AS ms", vec![local.into()]).await.unwrap().unwrap().try_get::<i64>("", "ms").unwrap()
        }
    };
    let next = super::tasks::rules::next_due(
        &db.db,
        &w,
        &rule,
        "Asia/Kolkata",
        at("2026-12-01 09:00").await,
        at("2026-12-24 11:00").await,
    )
    .await
    .unwrap();
    assert_eq!(next, Some(at("2026-12-28 10:00").await));
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn runs_fall_on_the_local_calendar_with_days_off_and_holidays() {
    let (db, _, w, _, _) = setup("allow").await;
    let ist = "Asia/Kolkata";
    let day = json!({"unit":"day","every":2,"at":"10:00"});
    assert_eq!(
        next(&db, &w, day, ist, "2026-11-24 09:00", "2026-11-24 11:00").await,
        Some(at(&db, "2026-11-26 10:00", ist).await)
    );
    let weekdays = json!({"unit":"week","days":[1,2,3,4,5],"at":"09:00"});
    assert_eq!(
        next(
            &db,
            &w,
            weekdays,
            ist,
            "2026-11-24 09:00",
            "2026-11-27 10:00"
        )
        .await,
        Some(at(&db, "2026-11-30 09:00", ist).await),
        "Friday to Monday"
    );
    let fortnight = json!({"unit":"week","every":2,"days":[2],"at":"09:00"});
    assert_eq!(
        next(
            &db,
            &w,
            fortnight,
            ist,
            "2026-11-24 08:00",
            "2026-11-24 10:00"
        )
        .await,
        Some(at(&db, "2026-12-08 09:00", ist).await)
    );
    let last = json!({"unit":"month","day_of_month":-1,"at":"18:00"});
    assert_eq!(
        next(
            &db,
            &w,
            last.clone(),
            ist,
            "2026-11-24 09:00",
            "2026-11-24 09:00"
        )
        .await,
        Some(at(&db, "2026-11-30 18:00", ist).await)
    );
    assert_eq!(
        next(&db, &w, last, ist, "2026-11-24 09:00", "2026-11-30 19:00").await,
        Some(at(&db, "2026-12-31 18:00", ist).await)
    );
    let thirty_first = json!({"unit":"month","day_of_month":31,"at":"09:00"});
    assert_eq!(
        next(
            &db,
            &w,
            thirty_first,
            ist,
            "2027-01-01 09:00",
            "2027-02-01 09:00"
        )
        .await,
        Some(at(&db, "2027-02-28 09:00", ist).await),
        "a short month runs on its last day"
    );
    let hourly = json!({"unit":"hour","from":"09:00","to":"19:00"});
    assert_eq!(
        next(
            &db,
            &w,
            hourly.clone(),
            ist,
            "2026-11-24 09:00",
            "2026-11-24 10:15"
        )
        .await,
        Some(at(&db, "2026-11-24 11:00", ist).await)
    );
    assert_eq!(
        next(&db, &w, hourly, ist, "2026-11-24 09:00", "2026-11-24 19:30").await,
        Some(at(&db, "2026-11-25 09:00", ist).await)
    );
    // Working days are Monday to Saturday by default: Sunday's run moves to Monday.
    let working = json!({"unit":"week","days":[7],"at":"10:00","working_days":true});
    assert_eq!(
        next(
            &db,
            &w,
            working.clone(),
            ist,
            "2026-11-24 09:00",
            "2026-11-28 11:00"
        )
        .await,
        Some(at(&db, "2026-11-30 10:00", ist).await)
    );
    execute(&db.db, "INSERT INTO relay_workspace_holidays(workspace,day,name) VALUES($1,'2026-11-30','Test holiday')", vec![w.clone().into()]).await.unwrap();
    assert_eq!(
        next(
            &db,
            &w,
            working.clone(),
            ist,
            "2026-11-24 09:00",
            "2026-11-28 11:00"
        )
        .await,
        Some(at(&db, "2026-12-01 10:00", ist).await),
        "past the holiday"
    );
    execute(
        &db.db,
        "UPDATE relay_workspaces SET holiday_rule='skip' WHERE id=$1",
        vec![w.clone().into()],
    )
    .await
    .unwrap();
    assert_eq!(
        next(
            &db,
            &w,
            working,
            ist,
            "2026-11-24 09:00",
            "2026-11-28 11:00"
        )
        .await,
        None,
        "every Sunday is skipped"
    );
    let daily = json!({"unit":"day","at":"10:00","working_days":true});
    assert_eq!(
        next(&db, &w, daily, ist, "2026-11-24 09:00", "2026-11-28 11:00").await,
        Some(at(&db, "2026-12-01 10:00", ist).await),
        "Sunday and the holiday are skipped"
    );
    // A minute rule is a fixed period from the first run (schema 145).
    let minutes = json!({"unit":"minute","every":90});
    assert_eq!(
        next(
            &db,
            &w,
            minutes.clone(),
            ist,
            "2026-11-24 09:00",
            "2026-11-24 12:05"
        )
        .await,
        Some(at(&db, "2026-11-24 13:30", ist).await)
    );
    assert_eq!(
        next(
            &db,
            &w,
            minutes,
            ist,
            "2026-11-24 09:00",
            "2026-11-24 08:00"
        )
        .await,
        Some(at(&db, "2026-11-24 09:00", ist).await),
        "before the first run, the first run"
    );
    // A daylight-saving zone keeps local clock time.
    let london = "Europe/London";
    let nine = json!({"unit":"day","at":"09:00"});
    assert_eq!(
        next(
            &db,
            &w,
            nine,
            london,
            "2027-03-20 09:00",
            "2027-03-27 10:00"
        )
        .await,
        Some(at(&db, "2027-03-28 09:00", london).await)
    );
    let utc = one(
        &db,
        "SELECT extract(hour FROM to_timestamp($1/1000.0) AT TIME ZONE 'UTC')::int AS h",
        vec![at(&db, "2027-03-28 09:00", london).await.into()],
    )
    .await;
    assert_eq!(utc["h"], 8, "09:00 in summer time is 08:00 UTC");
    let new_workspace = one(
        &db,
        "SELECT timezone,working_days FROM relay_workspaces WHERE id=$1",
        vec![w.clone().into()],
    )
    .await;
    assert_eq!(
        new_workspace["timezone"], "Asia/Kolkata",
        "India unless an admin chooses another"
    );
}

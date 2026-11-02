//! Follow-ups: a task that starts later is scheduled, wakes its agent for the same
//! person in the same conversation, waits while work runs, can't reach someone who
//! lost access (needs fixing), and closing cancels it. Late tasks show as late.
use super::super::super::control_tests::setup;
use super::super::super::tests::{chat, join, message, user};
use super::super::super::*;
use super::super::CreateTask;
use super::FollowUp;
use serde_json::json;

async fn one(db: &PgStore, sql: &str, args: Vec<sea_orm::Value>) -> Value {
    query(
        &db.db,
        &format!("SELECT to_jsonb(x) AS v FROM ({sql}) x"),
        args,
    )
    .await
    .unwrap()
    .map(|r| r.try_get("", "v").unwrap())
    .unwrap_or(Value::Null)
}

async fn status(db: &PgStore, w: &str, task: &str) -> String {
    one(
        db,
        "SELECT relay_task_status($1,$2) AS s",
        vec![w.into(), task.into()],
    )
    .await["s"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn wakeup(db: &PgStore, task: &str) -> Value {
    one(
        db,
        "SELECT state,reply,detail,due_ms FROM relay_task_wakeups WHERE task=$1 ORDER BY created_ms DESC LIMIT 1",
        vec![task.into()],
    )
    .await
}

async fn due_now(db: &PgStore, task: &str) {
    execute(
        &db.db,
        "UPDATE relay_task_wakeups SET due_ms=1 WHERE task=$1 AND state='pending'",
        vec![task.into()],
    )
    .await
    .unwrap();
}

/// Claim a pooled request as an executor would.
async fn claim(db: &PgStore, reply: &str) -> crate::identity::chat::model::Job {
    let payload = one(
        db,
        "SELECT payload FROM relay_chat_jobs WHERE reply=$1",
        vec![reply.into()],
    )
    .await;
    let mut job: crate::identity::chat::model::Job =
        serde_json::from_value(payload["payload"].clone()).unwrap();
    job.claim = Some(uuid::Uuid::new_v4().to_string());
    execute(&db.db, "UPDATE relay_chat_jobs SET state='running',claim=$2,attempts=attempts+1,lease_until=floor(extract(epoch FROM clock_timestamp()))::bigint+300 WHERE reply=$1 AND state='queued'", vec![reply.into(), job.claim.clone().into()]).await.unwrap();
    job
}

fn later(title: &str, start: i64) -> CreateTask {
    serde_json::from_value(
        json!({"title": title, "start_in_minutes": start, "due_in_minutes": 2 * start}),
    )
    .unwrap()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_follow_up_wakes_the_agent_for_the_same_person_and_closing_cancels_it() {
    let (mut db, owner, w, _, job) = setup("allow").await;
    db.pooled = true;
    let task = db
        .create_task(&job, &later("Call Acme back", 60))
        .await
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    db.chat_finish(&job, "I'll call Acme back in an hour", "complete")
        .await
        .unwrap();
    assert_eq!(status(&db, &w, &task).await, "scheduled");
    let open = db.tasks_open(&owner, &w, None).await.unwrap();
    assert!(open["items"][0]["next_wakeup_ms"].as_i64().unwrap() > 0);
    assert!(
        open["items"][0]["due_ms"].as_i64().unwrap()
            > open["items"][0]["next_wakeup_ms"].as_i64().unwrap()
    );
    assert_eq!(db.dispatch_task_wakeups().await.unwrap(), 0, "not due yet");

    due_now(&db, &task).await;
    assert_eq!(db.dispatch_task_wakeups().await.unwrap(), 1);
    let woke = wakeup(&db, &task).await;
    assert_eq!(woke["state"], "started");
    let reply = woke["reply"].as_str().unwrap().to_string();
    let request = one(
        &db,
        "SELECT j.task,j.payload->>'chat' AS chat,j.payload->>'requester' AS asker,j.payload->>'prompt' AS content FROM relay_chat_jobs j WHERE j.reply=$1",
        vec![reply.clone().into()],
    )
    .await;
    assert_eq!(
        request["task"],
        task.as_str(),
        "the request is part of the task"
    );
    assert_eq!(
        request["chat"],
        job.chat.as_str(),
        "in the task's own conversation"
    );
    assert_eq!(
        request["asker"],
        owner.id.as_str(),
        "for the person it promised"
    );
    assert!(
        request["content"]
            .as_str()
            .unwrap()
            .contains("Follow-up · Call Acme back")
    );
    assert_eq!(status(&db, &w, &task).await, "working");
    assert_eq!(
        db.dispatch_task_wakeups().await.unwrap(),
        0,
        "a follow-up starts once"
    );

    // The agent books another check; while its current work runs, a due one waits.
    let next = claim(&db, &reply).await;
    let follow: FollowUp =
        serde_json::from_value(json!({"task": task, "in_minutes": 30, "note": "Ask about the PO"}))
            .unwrap();
    db.follow_up(&next, &follow).await.unwrap();
    due_now(&db, &task).await;
    assert_eq!(db.dispatch_task_wakeups().await.unwrap(), 0);
    let waiting = wakeup(&db, &task).await;
    assert_eq!(waiting["state"], "pending");
    assert!(
        waiting["due_ms"].as_i64().unwrap() > 0,
        "pushed back five minutes"
    );
    assert!(waiting["detail"].as_str().unwrap().contains("current work"));

    // Closing the task cancels its follow-up.
    let close = serde_json::from_value(
        json!({"task": task, "outcome": "done", "result": "Acme confirmed the PO"}),
    )
    .unwrap();
    db.close_task(&next, &close).await.unwrap();
    assert_eq!(wakeup(&db, &task).await["state"], "cancelled");
    assert!(
        db.follow_up(&next, &follow).await.is_err(),
        "a closed task takes no follow-up"
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_follow_up_that_cannot_reach_the_person_needs_fixing_and_late_tasks_show_late() {
    let (mut db, owner, w, _, job) = setup("allow").await;
    db.pooled = true;
    let member = user(&db, "follow-up-member").await;
    join(&db, &owner, &w, &member, "Member").await;
    let room = chat(
        &db,
        &owner,
        &w,
        vec![member.id.clone()],
        vec![job.agent.clone()],
    )
    .await;
    let sent = db
        .chat_send(
            &member,
            &w,
            &room,
            &message("Chase the courier tomorrow", Some(&job.agent)),
        )
        .await
        .unwrap()
        .unwrap();
    db.chat_finish(&job, "Done", "complete").await.unwrap();
    let asked = claim(&db, &sent.reply).await;
    // Only a promise takes a follow-up; ranges are checked.
    assert!(
        db.follow_up(
            &asked,
            &serde_json::from_value(json!({"task":"none","in_minutes":5})).unwrap()
        )
        .await
        .is_err()
    );
    assert!(
        db.create_task(&asked, &later("Chase the courier", 0))
            .await
            .is_err()
    );
    let task = db
        .create_task(&asked, &later("Chase the courier", 1440))
        .await
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    db.chat_finish(&asked, "I'll chase the courier tomorrow", "complete")
        .await
        .unwrap();

    // The person leaves the conversation: the follow-up can't run for them.
    execute(
        &db.db,
        "DELETE FROM relay_chat_people WHERE chat=$1 AND user_id=$2",
        vec![room.clone().into(), member.id.clone().into()],
    )
    .await
    .unwrap();
    due_now(&db, &task).await;
    assert_eq!(db.dispatch_task_wakeups().await.unwrap(), 0);
    let blocked = wakeup(&db, &task).await;
    assert_eq!(blocked["state"], "blocked");
    assert!(
        blocked["detail"]
            .as_str()
            .unwrap()
            .starts_with("Follow-up blocked")
    );
    assert_eq!(status(&db, &w, &task).await, "needs_fixing");
    let open = db.tasks_open(&owner, &w, None).await.unwrap();
    let item = open["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == task.as_str())
        .unwrap();
    assert!(
        item["blocked_wakeup"]
            .as_str()
            .unwrap()
            .starts_with("Follow-up blocked")
    );

    // A promise past its due time with nothing booked is late.
    execute(
        &db.db,
        "UPDATE relay_task_wakeups SET state='cancelled' WHERE task=$1",
        vec![task.clone().into()],
    )
    .await
    .unwrap();
    execute(
        &db.db,
        "UPDATE relay_tasks SET due_ms=1 WHERE id=$1",
        vec![task.clone().into()],
    )
    .await
    .unwrap();
    assert_eq!(status(&db, &w, &task).await, "late");
}

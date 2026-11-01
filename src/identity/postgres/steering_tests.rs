//! Adding to running work reaches the agent at its next step, and is never lost;
//! a routine proposed in Chat is saved only when the person who asked confirms it.
use super::super::super::super::control_tests::setup;
use super::super::super::super::tests::{join, user};
use super::super::super::super::*;
use super::Proposal;
use serde_json::json;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn what_the_person_adds_reaches_the_running_request_at_its_next_step_and_is_never_lost() {
    let (mut db, owner, w, _, job) = setup("allow").await;
    db.pooled = true;
    let member = user(&db, "steer-member").await;
    join(&db, &owner, &w, &member, "Member").await;
    assert!(
        db.steer_request(&member, &w, &job.reply, "Use my discount")
            .await
            .is_err(),
        "only the person who asked"
    );
    db.steer_request(&owner, &w, &job.reply, "Also use the new price list")
        .await
        .unwrap();

    // The agent's next tool call carries it, once.
    let next = crate::identity::control::broker::invoke(&db, &job, "open_tasks", json!({}))
        .await
        .unwrap();
    assert_eq!(
        next["person_added"][0]["text"],
        "Also use the new price list"
    );
    let after = crate::identity::control::broker::invoke(&db, &job, "open_tasks", json!({}))
        .await
        .unwrap();
    assert!(after["person_added"].is_null());
    let details = db.activity_request(&owner, &w, &job.reply).await.unwrap();
    assert_eq!(details["added"][0]["seen"], true);

    // Something added that the agent never saw becomes a new request when it finishes.
    db.steer_request(&owner, &w, &job.reply, "And copy Ravi on the email")
        .await
        .unwrap();
    db.chat_finish(&job, "Updated with the new prices", "complete")
        .await
        .unwrap();
    let carried: String = query(
        &db.db,
        "SELECT carried FROM relay_job_steers WHERE content='And copy Ravi on the email'",
        vec![],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "carried")
    .unwrap();
    let prompt: String = query(
        &db.db,
        "SELECT payload->>'prompt' AS p FROM relay_chat_jobs WHERE reply=$1",
        vec![carried.into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "p")
    .unwrap();
    assert!(prompt.contains("And copy Ravi on the email"), "{prompt}");
    assert!(
        db.steer_request(&owner, &w, &job.reply, "Too late")
            .await
            .is_err(),
        "a finished request takes no additions"
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn adding_to_a_task_reaches_its_running_request() {
    let (mut db, owner, w, _, job) = setup("allow").await;
    db.pooled = true;
    let task = db
        .create_task(
            &job,
            &serde_json::from_value(json!({"title":"Send quotes to Acme"})).unwrap(),
        )
        .await
        .unwrap();
    let id = task["id"].as_str().unwrap();
    db.steer_task(&owner, &w, id, "Include the bulk discount")
        .await
        .unwrap();
    let next = crate::identity::control::broker::invoke(&db, &job, "open_tasks", json!({}))
        .await
        .unwrap();
    assert_eq!(next["person_added"][0]["text"], "Include the bulk discount");
    db.chat_finish(&job, "I'll send them tomorrow", "complete")
        .await
        .unwrap();
    assert!(
        db.steer_task(&owner, &w, id, "Anything").await.is_err(),
        "nothing running for it now"
    );
}

async fn count(db: &PgStore, id: &str) -> i64 {
    query(
        &db.db,
        "SELECT count(*) AS n FROM relay_agent_schedules WHERE id=$1",
        vec![id.into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "n")
    .unwrap()
}

fn proposal(name: &str) -> Proposal {
    serde_json::from_value(json!({"name": name, "instructions": "Send me last week's sales summary", "first_in_minutes": 600, "every_minutes": 10080})).unwrap()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_routine_proposed_in_chat_is_saved_only_when_the_person_who_asked_confirms_it() {
    let (mut db, owner, w, _, job) = setup("allow").await;
    db.pooled = true;
    let member = user(&db, "routine-member").await;
    join(&db, &owner, &w, &member, "Member").await;
    let first = db
        .propose_schedule(&job, &proposal("Monday sales summary"))
        .await
        .unwrap()["draft"]
        .as_str()
        .unwrap()
        .to_string();
    let second = db
        .propose_schedule(&job, &proposal("Friday stock check"))
        .await
        .unwrap()["draft"]
        .as_str()
        .unwrap()
        .to_string();
    db.propose_schedule(&job, &proposal("Third")).await.unwrap();
    assert!(
        db.propose_schedule(&job, &proposal("Fourth"))
            .await
            .is_err(),
        "three per request"
    );
    assert!(
        db.propose_schedule(
            &job,
            &serde_json::from_value(
                json!({"name":"x","instructions":"y","first_in_minutes":5,"every_minutes":5})
            )
            .unwrap()
        )
        .await
        .is_err()
    );
    db.chat_finish(&job, "I can send that every Monday", "complete")
        .await
        .unwrap();

    // Chat shows the proposal under the reply; nothing runs until it's saved.
    let page = db.chat_page(&owner, &w, &job.chat, None).await.unwrap();
    let reply = page["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == job.reply.as_str())
        .unwrap()
        .clone();
    assert_eq!(reply["routines"][0]["name"], "Monday sales summary");
    assert_eq!(reply["routines"][0]["state"], "pending");
    assert_eq!(count(&db, &first).await, 0);

    assert!(
        db.decide_schedule_draft(&member, &w, &first, true)
            .await
            .is_err(),
        "only the person who asked"
    );
    assert_eq!(
        db.decide_schedule_draft(&owner, &w, &first, true)
            .await
            .unwrap()["state"],
        "saved"
    );
    assert_eq!(
        db.decide_schedule_draft(&owner, &w, &first, true)
            .await
            .unwrap()["state"],
        "saved",
        "a retry is the same routine"
    );
    assert_eq!(count(&db, &first).await, 1);
    let interval: i64 = query(
        &db.db,
        "SELECT interval_seconds FROM relay_agent_schedules WHERE id=$1",
        vec![first.into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "interval_seconds")
    .unwrap();
    assert_eq!(interval, 604_800);
    assert_eq!(
        db.decide_schedule_draft(&owner, &w, &second, false)
            .await
            .unwrap()["state"],
        "dismissed"
    );
    assert_eq!(count(&db, &second).await, 0);
}

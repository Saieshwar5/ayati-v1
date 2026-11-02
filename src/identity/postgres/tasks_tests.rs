//! Tasks: automatic tasks from writes and waits, an agent's promises, a person's
//! "Make this a task" and closing, status from the records, and who sees what.
use super::super::control_tests::{request, setup};
use super::super::tests::{chat, join, message, user};
use super::super::*;
use super::{AttachTask, CloseTask, CreateTask};
use serde_json::json;

async fn task_of(db: &PgStore, reply: &str) -> Option<String> {
    query(
        &db.db,
        "SELECT task FROM relay_chat_jobs WHERE reply=$1",
        vec![reply.into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "task")
    .unwrap()
}

async fn status(db: &PgStore, w: &str, task: &str) -> String {
    query(
        &db.db,
        "SELECT relay_task_status($1,$2) AS s",
        vec![w.into(), task.into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "s")
    .unwrap()
}

fn create(title: &str) -> CreateTask {
    serde_json::from_value(json!({"title": title, "done_when": "Acme has both quotes"})).unwrap()
}

async fn open_ids(db: &PgStore, person: &User, w: &str) -> Vec<String> {
    db.tasks_open(person, w, None).await.unwrap()["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_write_makes_a_task_that_closes_with_its_request_and_shows_in_chat() {
    let (db, owner, w, connection, job) = setup("allow").await;
    assert_eq!(
        task_of(&db, &job.reply).await,
        None,
        "a request alone is not a task"
    );
    let first = db
        .control_propose(&job, &request(&connection, "A-1"))
        .await
        .unwrap();
    let task = task_of(&db, &job.reply)
        .await
        .expect("a write makes a task");
    assert_eq!(status(&db, &w, &task).await, "working");
    assert_eq!(open_ids(&db, &owner, &w).await, vec![task.clone()]);
    // A second write in the same request (after the first finished) stays in the same task.
    let dispatch = first.dispatch.unwrap();
    db.control_ready(&w, &dispatch).await.unwrap();
    db.control_finish(&w, &dispatch, json!({"updated": true}), true)
        .await
        .unwrap();
    db.control_propose(&job, &request(&connection, "A-2"))
        .await
        .unwrap();
    assert_eq!(task_of(&db, &job.reply).await, Some(task.clone()));
    db.chat_finish(&job, "Updated both orders", "complete")
        .await
        .unwrap();
    assert_eq!(status(&db, &w, &task).await, "done");
    assert!(
        open_ids(&db, &owner, &w).await.is_empty(),
        "done tasks leave Open"
    );
    // The reply in Chat carries its task for the chip.
    let page = db.chat_page(&owner, &w, &job.chat, None).await.unwrap();
    let reply = page["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == job.reply.as_str())
        .unwrap()
        .clone();
    assert_eq!(reply["task"]["id"], task.as_str());
    assert_eq!(reply["task"]["status"], "done");
    assert_eq!(reply["task"]["title"], "Update the sample order");
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn an_agents_promise_stays_open_takes_more_work_and_closes_when_done() {
    let (db, owner, w, connection, job) = setup("allow").await;
    // The write already opened an automatic task: the promise takes it over.
    db.control_propose(&job, &request(&connection, "B-1"))
        .await
        .unwrap();
    let automatic = task_of(&db, &job.reply).await.unwrap();
    let saved = db
        .create_task(&job, &create("Send quotes to Acme"))
        .await
        .unwrap();
    assert_eq!(saved["id"], automatic.as_str(), "one task, not two");
    // Repeating the same promise is harmless; a different one in this request is refused.
    db.create_task(&job, &create("Send quotes to Acme"))
        .await
        .unwrap();
    assert!(
        db.create_task(&job, &create("Something else"))
            .await
            .is_err()
    );
    db.chat_finish(&job, "I'll send both quotes tomorrow", "complete")
        .await
        .unwrap();
    assert_eq!(
        status(&db, &w, &automatic).await,
        "open",
        "a promise stays open after its reply"
    );
    assert_eq!(open_ids(&db, &owner, &w).await, vec![automatic.clone()]);

    // The next request about it attaches instead of promising again.
    db.chat_send(
        &owner,
        &w,
        &job.chat,
        &message("Did the quotes go out?", Some(&job.agent)),
    )
    .await
    .unwrap();
    let next = db.claim_job().await.unwrap().unwrap();
    let listed = db.open_tasks(&next).await.unwrap();
    assert_eq!(listed["tasks"][0]["id"], automatic.as_str());
    db.attach_to_task(
        &next,
        &serde_json::from_value::<AttachTask>(json!({"task": automatic})).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(status(&db, &w, &automatic).await, "working");
    let close: CloseTask = serde_json::from_value(
        json!({"task": automatic, "outcome": "done", "result": "Both quotes sent to Acme"}),
    )
    .unwrap();
    db.close_task(&next, &close).await.unwrap();
    db.close_task(&next, &close).await.unwrap();
    db.chat_finish(&next, "Both quotes went out", "complete")
        .await
        .unwrap();
    assert_eq!(status(&db, &w, &automatic).await, "done");
    let details = db.task_details(&owner, &w, &automatic).await.unwrap();
    assert_eq!(details["requests"].as_array().unwrap().len(), 2);
    assert_eq!(details["result"], "Both quotes sent to Acme");
    assert!(open_ids(&db, &owner, &w).await.is_empty());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_question_makes_a_waiting_task_and_a_failed_promise_needs_fixing() {
    let (db, owner, w, _, job) = setup("allow").await;
    let ask = serde_json::from_value(json!({"key":"route","specification":{"version":1,"kind":"single_choice","title":"Choose a delivery method","options":[{"id":"air","label":"Air"},{"id":"road","label":"Road"}]}})).unwrap();
    db.ask_question(&job, &ask).await.unwrap();
    let task = task_of(&db, &job.reply)
        .await
        .expect("waiting for a person makes a task");
    execute(
        &db.db,
        "UPDATE relay_chat_jobs SET state='waiting' WHERE reply=$1",
        vec![job.reply.clone().into()],
    )
    .await
    .unwrap();
    assert_eq!(status(&db, &w, &task).await, "waiting");
    let open = db.tasks_open(&owner, &w, None).await.unwrap();
    assert_eq!(open["items"][0]["waiting_on"], "question");

    // A promise whose latest request failed needs fixing (its own request: one with
    // an unanswered question goes back to waiting when it finishes).
    db.chat_send(
        &owner,
        &w,
        &job.chat,
        &message("Book the courier for Friday", Some(&job.agent)),
    )
    .await
    .unwrap();
    let booking = db.claim_job().await.unwrap().unwrap();
    let promise = db
        .create_task(&booking, &create("Book the courier"))
        .await
        .unwrap();
    db.chat_finish(&booking, "The courier site refused the booking", "failed")
        .await
        .unwrap();
    assert_eq!(
        status(&db, &w, promise["id"].as_str().unwrap()).await,
        "needs_fixing"
    );
    let open = db.tasks_open(&owner, &w, None).await.unwrap();
    assert_eq!(
        open["items"][0]["id"], promise["id"],
        "needs fixing comes first"
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn people_make_and_close_tasks_and_only_those_who_may_see_one_do() {
    let (db, owner, w, _, job) = setup("allow").await;
    let member = user(&db, "tasks-member").await;
    let outsider = user(&db, "tasks-outsider").await;
    join(&db, &owner, &w, &member, "Member").await;
    join(&db, &owner, &w, &outsider, "Member").await;
    db.chat_finish(&job, "Here is the sample order", "complete")
        .await
        .unwrap();
    assert_eq!(task_of(&db, &job.reply).await, None);
    // Someone who can't see the request can't make it a task.
    assert!(
        db.task_from_request(&outsider, &w, &job.reply, None)
            .await
            .is_err()
    );
    let made = db
        .task_from_request(
            &owner,
            &w,
            &job.reply,
            Some("Follow up on the sample order"),
        )
        .await
        .unwrap();
    let task = made["id"].as_str().unwrap().to_string();
    assert_eq!(task_of(&db, &job.reply).await, Some(task.clone()));
    assert_eq!(status(&db, &w, &task).await, "open");
    assert!(
        open_ids(&db, &outsider, &w).await.is_empty(),
        "not in the conversation"
    );
    assert!(db.task_details(&outsider, &w, &task).await.is_err());

    // In the conversation, a member may say it's not a task but may not close it as done.
    let room = chat(
        &db,
        &owner,
        &w,
        vec![member.id.clone()],
        vec![job.agent.clone()],
    )
    .await;
    execute(
        &db.db,
        "UPDATE relay_tasks SET chat=$2 WHERE id=$1",
        vec![task.clone().into(), room.into()],
    )
    .await
    .unwrap();
    assert_eq!(open_ids(&db, &member, &w).await, vec![task.clone()]);
    assert!(
        db.task_close(&member, &w, &task, "done", None)
            .await
            .is_err()
    );
    db.task_close(&member, &w, &task, "not_task", None)
        .await
        .unwrap();
    assert_eq!(status(&db, &w, &task).await, "not_task");
    assert!(open_ids(&db, &owner, &w).await.is_empty());
    let page = db.chat_page(&owner, &w, &job.chat, None).await.unwrap();
    let reply = page["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == job.reply.as_str())
        .unwrap()
        .clone();
    assert!(
        reply["task"].is_null(),
        "no chip for something that isn't a task"
    );
    // Closing again with the same outcome is harmless; another outcome is refused.
    db.task_close(&member, &w, &task, "not_task", None)
        .await
        .unwrap();
    assert!(
        db.task_close(&owner, &w, &task, "done", None)
            .await
            .is_err()
    );
}

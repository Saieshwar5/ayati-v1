//! Tasks with a lifetime (schema 144): every run booked by Cuckoo, kept quiet when
//! nothing changed, skipped when missed or overlapping, paused after three failures
//! and ended or given up at their limits. Shared helpers for the change tests.
use super::super::control_tests::setup;
use super::super::*;
use super::CreateTask;
use serde_json::json;

pub(in crate::identity::postgres) async fn one(
    db: &PgStore,
    sql: &str,
    args: Vec<sea_orm::Value>,
) -> Value {
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

/// Milliseconds of a local time in `zone`.
pub(in crate::identity::postgres) async fn at(db: &PgStore, local: &str, zone: &str) -> i64 {
    one(
        db,
        "SELECT (extract(epoch FROM ($1::timestamp AT TIME ZONE $2))*1000)::bigint AS ms",
        vec![local.into(), zone.into()],
    )
    .await["ms"]
        .as_i64()
        .unwrap()
}

pub(super) async fn status(db: &PgStore, w: &str, task: &str) -> String {
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

pub(super) async fn wakeups(db: &PgStore, task: &str) -> Vec<Value> {
    let rows = one(db, "SELECT coalesce(jsonb_agg(to_jsonb(k) ORDER BY k.created_ms,k.due_ms),'[]') AS all FROM relay_task_wakeups k WHERE task=$1", vec![task.into()]).await;
    rows["all"].as_array().unwrap().clone()
}

pub(super) async fn pending(db: &PgStore, task: &str) -> Vec<Value> {
    wakeups(db, task)
        .await
        .into_iter()
        .filter(|k| k["state"] == "pending")
        .collect()
}

pub(super) async fn due_now(db: &PgStore, task: &str) {
    execute(&db.db, "UPDATE relay_task_wakeups SET due_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint-1000 WHERE task=$1 AND state='pending'", vec![task.into()]).await.unwrap();
}

/// Claim a pooled request as an executor would.
pub(super) async fn claim(db: &PgStore, reply: &str) -> crate::identity::chat::model::Job {
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

pub(super) async fn last_run(db: &PgStore, task: &str) -> (String, String) {
    let k = wakeups(db, task)
        .await
        .into_iter()
        .rev()
        .find(|k| k["state"] == "started" && !k["reply"].is_null())
        .unwrap();
    let reply = k["reply"].as_str().unwrap().to_string();
    let content = one(
        db,
        "SELECT content FROM relay_chat_messages WHERE id=$1",
        vec![k["message"].clone().as_str().unwrap().into()],
    )
    .await;
    (
        reply,
        content["content"].as_str().unwrap_or_default().to_string(),
    )
}

pub(super) fn task(value: Value) -> CreateTask {
    serde_json::from_value(value).unwrap()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn cuckoo_books_every_run_and_a_quiet_run_stays_out_of_the_conversation() {
    let (mut db, owner, w, _, job) = setup("allow").await;
    db.pooled = true;
    let tools = db.open_tasks(&job).await.unwrap();
    assert_eq!(
        tools["timezone"], "Asia/Kolkata",
        "the agent is told the time"
    );
    assert!(tools["now"].as_str().unwrap().len() >= 16);
    let made = db
        .create_task(&job, &task(json!({"title":"Check stock below reorder level","repeat":{"unit":"day","at":"10:00"},
            "instructions":"List the items below their reorder level and tell purchasing"})))
        .await
        .unwrap();
    let id = made["id"].as_str().unwrap().to_string();
    assert_eq!(made["task"]["rule_text"], "Every day at 10:00");
    assert!(
        made["task"]["next_text"]
            .as_str()
            .unwrap()
            .ends_with("10:00")
    );
    db.chat_finish(
        &job,
        "Done. I'll check stock every day at 10:00.",
        "complete",
    )
    .await
    .unwrap();
    assert_eq!(status(&db, &w, &id).await, "scheduled");
    assert_eq!(db.dispatch_task_wakeups().await.unwrap(), 0, "not due yet");

    due_now(&db, &id).await;
    assert_eq!(db.dispatch_task_wakeups().await.unwrap(), 1);
    let (reply, content) = last_run(&db, &id).await;
    assert!(content.starts_with("Task run 1 · Check stock"), "{content}");
    assert!(content.contains("first run"));
    assert!(
        content.contains("What to do each run: List the items below their reorder level"),
        "{content}"
    );
    let booked = pending(&db, &id).await;
    assert_eq!(booked.len(), 1, "the next run is booked as this one starts");
    assert_eq!(booked[0]["kind"], "repeat");
    let row = one(&db, "SELECT runs,(SELECT task FROM relay_chat_jobs WHERE reply=$2) AS job_task,(SELECT chat FROM relay_chat_messages WHERE id=$2) AS chat FROM relay_tasks WHERE id=$1", vec![id.clone().into(), reply.clone().into()]).await;
    assert_eq!(row["runs"], 1);
    assert_eq!(row["job_task"], id.as_str(), "the run is part of the task");
    // The owner's one rule (schema 151): the run happens in the group that asked.
    assert_eq!(
        row["chat"],
        job.chat.as_str(),
        "the run works in the task's group"
    );

    let run = claim(&db, &reply).await;
    db.task_note(
        &run,
        &serde_json::from_value(json!({"task": id, "note": "Rice and sugar are fine"})).unwrap(),
    )
    .await
    .unwrap();
    db.chat_finish(&run, "NO_CHANGE", "complete").await.unwrap();
    let quiet = one(&db, "SELECT k.quiet,m.state AS reply_state,r.state AS request_state FROM relay_task_wakeups k JOIN relay_chat_messages m ON m.id=k.reply JOIN relay_chat_messages r ON r.id=k.message WHERE k.reply=$1", vec![reply.clone().into()]).await;
    assert_eq!(quiet["quiet"], true);
    assert_eq!(
        quiet["reply_state"], "deleted",
        "nothing to tell: kept out of the conversation"
    );
    assert_eq!(quiet["request_state"], "deleted");

    due_now(&db, &id).await;
    assert_eq!(db.dispatch_task_wakeups().await.unwrap(), 1);
    let (_, second) = last_run(&db, &id).await;
    assert!(second.starts_with("Task run 2"), "{second}");
    assert!(
        second.contains("Rice and sugar are fine"),
        "notes reach the next run"
    );
    let (reply, _) = last_run(&db, &id).await;
    let run = claim(&db, &reply).await;
    db.chat_finish(
        &run,
        "Rice is below its reorder level: 12 bags left.",
        "complete",
    )
    .await
    .unwrap();
    let answer = one(
        &db,
        "SELECT chat,content,kind FROM relay_chat_messages WHERE id=$1",
        vec![reply.clone().into()],
    )
    .await;
    assert_eq!(
        (
            answer["chat"].as_str(),
            answer["content"].as_str(),
            answer["kind"].as_str()
        ),
        (
            Some(job.chat.as_str()),
            Some("Rice is below its reorder level: 12 bags left."),
            Some("agent")
        ),
        "the reply is in the group that asked; nothing is copied"
    );
    let summary = db.task_details(&owner, &w, &id).await.unwrap();
    assert!(summary["id"].is_string());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn missed_and_overlapping_runs_are_skipped_and_the_next_is_booked() {
    let (mut db, _, w, _, job) = setup("allow").await;
    db.pooled = true;
    let id = db
        .create_task(
            &job,
            &task(json!({"title":"Post sales in #sales","repeat":{"unit":"hour"}})),
        )
        .await
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    db.chat_finish(&job, "Done", "complete").await.unwrap();
    due_now(&db, &id).await;
    assert_eq!(db.dispatch_task_wakeups().await.unwrap(), 1);
    // The run is still going when the next is due: skipped, the one after booked.
    due_now(&db, &id).await;
    assert_eq!(db.dispatch_task_wakeups().await.unwrap(), 0);
    let all = wakeups(&db, &id).await;
    assert!(
        all.iter()
            .any(|k| k["state"] == "skipped"
                && k["detail"].as_str().unwrap().contains("still going"))
    );
    assert_eq!(pending(&db, &id).await.len(), 1);
    // Cuckoo was down for hours: the old run is skipped, not run late.
    let (reply, _) = last_run(&db, &id).await;
    let run = claim(&db, &reply).await;
    db.chat_finish(&run, "Posted", "complete").await.unwrap();
    execute(&db.db, "UPDATE relay_task_wakeups SET due_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint-7200000 WHERE task=$1 AND state='pending'", vec![id.clone().into()]).await.unwrap();
    assert_eq!(db.dispatch_task_wakeups().await.unwrap(), 0);
    assert!(wakeups(&db, &id).await.iter().any(|k| k["state"] == "skipped" && k["detail"].as_str().unwrap().starts_with("Missed")));
    let next = pending(&db, &id).await;
    assert_eq!(next.len(), 1);
    let now = super::runs::now_ms(&db.db).await.unwrap();
    assert!(next[0]["due_ms"].as_i64().unwrap() > now);
    assert_eq!(status(&db, &w, &id).await, "scheduled");
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn three_failed_runs_pause_a_task_and_a_task_waiting_for_something_gives_up() {
    let (mut db, owner, w, _, job) = setup("allow").await;
    db.pooled = true;
    let flaky = db
        .create_task(
            &job,
            &task(json!({"title":"Back up the price list","repeat":{"unit":"day","at":"20:00"}})),
        )
        .await
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let chase = db.create_task(&job, &task(json!({"title":"Chase invoice 311","done_when":"Invoice 311 is paid","repeat":{"unit":"day","every":2,"at":"10:00"},"max_runs":2}))).await;
    // The request that made them is part of the first task; finish it before runs.
    db.chat_finish(&job, "Done", "complete").await.unwrap();
    for _ in 0..3 {
        due_now(&db, &flaky).await;
        assert_eq!(db.dispatch_task_wakeups().await.unwrap(), 1);
        let (reply, _) = last_run(&db, &flaky).await;
        let run = claim(&db, &reply).await;
        db.chat_finish(&run, "The sheet is unavailable", "failed")
            .await
            .unwrap();
    }
    due_now(&db, &flaky).await;
    assert_eq!(db.dispatch_task_wakeups().await.unwrap(), 0);
    assert_eq!(status(&db, &w, &flaky).await, "paused");
    assert!(
        pending(&db, &flaky).await.is_empty(),
        "nothing runs while paused"
    );
    let resumed = db.task_set_paused(&owner, &w, &flaky, false).await.unwrap();
    assert_eq!(resumed["paused"], false);
    assert_eq!(
        pending(&db, &flaky).await.len(),
        1,
        "resuming books the next run"
    );

    let chase = chase.unwrap_err().to_string();
    assert!(
        chase.contains("already works towards"),
        "one request makes one task: {chase}"
    );
    let ask = super::super::tests::message("Chase invoice 311 until it is paid", Some(&job.agent));
    db.pooled = false;
    db.chat_send(&owner, &w, &job.chat, &ask).await.unwrap();
    let asked = db.claim_job().await.unwrap().unwrap();
    db.pooled = true;
    let chase = db.create_task(&asked, &task(json!({"title":"Chase invoice 311","done_when":"Invoice 311 is paid","repeat":{"unit":"day","every":2,"at":"10:00"},"max_runs":2}))).await.unwrap();
    assert!(
        chase["task"]["rule_text"]
            .as_str()
            .unwrap()
            .contains("gives up after 2 runs"),
        "{}",
        chase["task"]
    );
    let chase = chase["id"].as_str().unwrap().to_string();
    db.chat_finish(&asked, "Done", "complete").await.unwrap();
    for n in 1..=2 {
        due_now(&db, &chase).await;
        assert_eq!(db.dispatch_task_wakeups().await.unwrap(), 1);
        let (reply, content) = last_run(&db, &chase).await;
        assert!(content.contains(&format!("run {n} of 2")), "{content}");
        if n == 2 {
            assert!(content.contains("last run"));
            let left = pending(&db, &chase).await;
            assert_eq!(left.len(), 1);
            assert_eq!(left[0]["kind"], "end", "no third run: an end check");
            assert_eq!(
                db.dispatch_task_wakeups().await.unwrap(),
                0,
                "it waits for the last run"
            );
            assert_eq!(status(&db, &w, &chase).await, "working");
        }
        let run = claim(&db, &reply).await;
        db.chat_finish(&run, "Sent the reminder", "complete")
            .await
            .unwrap();
    }
    due_now(&db, &chase).await;
    db.dispatch_task_wakeups().await.unwrap();
    let closed = one(
        &db,
        "SELECT outcome,result FROM relay_tasks WHERE id=$1",
        vec![chase.clone().into()],
    )
    .await;
    assert_eq!(closed["outcome"], "failed");
    assert_eq!(
        closed["result"],
        "Gave up after 2 runs: Invoice 311 is paid"
    );
}

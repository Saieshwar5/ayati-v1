//! Lost attempts resume from controller records; uncertain actions wait.
use super::{
    executor_slot_tests::reserve, executor_task_tests::executor, reply_output_tests::begin,
    worker_control::Work, *,
};
use crate::identity::provider::digest;
use serde_json::json;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_retry_after_a_lost_attempt_continues_from_what_it_had_written() {
    let (db, _, _, _, hash, mut update) = begin().await;
    update.phase = "responding".into();
    update.text = "Step one: checked the order history.\n".into();
    db.append_output(&update, &hash).await.unwrap();
    let reply = update.assignment.scope.reply.clone();
    // The executor is lost: its lease expires and another executor takes the task.
    execute(
        &db.db,
        "UPDATE relay_chat_jobs SET lease_until=0 WHERE reply=$1",
        vec![reply.clone().into()],
    )
    .await
    .unwrap();
    let (next, slot) = executor(&db).await;
    let scope = reserve(&db, &next).await.unwrap();
    assert_eq!(scope.reply, reply);
    let record = db
        .controlled_assignment(&scope, &digest(&slot.token))
        .await
        .unwrap();
    let Work::Reply(assignment) = record.work else {
        panic!("expected a chat reply")
    };
    let context = crate::identity::chat::execution::prepare(&db, &assignment.job, 45)
        .await
        .unwrap();
    assert!(context.request.contains("Interrupted earlier attempt"));
    assert!(
        context
            .request
            .contains("Step one: checked the order history.")
    );
    assert!(context.request.contains("do not repeat them"));
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_lost_attempt_with_an_executing_action_waits_then_resumes_from_a_checkpoint() {
    let (db, _, w, connection, job) = super::control_tests::setup("allow").await;
    let proposed = db
        .control_propose(&job, &super::control_tests::request(&connection, "one"))
        .await
        .unwrap();
    assert_eq!(proposed.action["state"], "executing");
    // Model the job as a pooled attempt that streamed text, then lost its lease.
    let lost = uuid::Uuid::new_v4().to_string();
    execute(
        &db.db,
        "UPDATE relay_chat_jobs SET dispatch='pooled',generation=1,attempts=1 WHERE reply=$1",
        vec![job.reply.clone().into()],
    )
    .await
    .unwrap();
    execute(&db.db,"INSERT INTO relay_worker_assignments(id,workspace,reply,generation,provider,state,outcome,lease_seconds,finished_ms) VALUES($1,$2,$3,1,'local','lost','lost',15,1)",
        vec![lost.clone().into(),w.clone().into(),job.reply.clone().into()]).await.unwrap();
    execute(&db.db,"INSERT INTO relay_assignment_output(assignment,sequence,fingerprint,phase,text) VALUES($1,1,$2,'responding','Updating order one now.')",
        vec![lost.into(),"f".repeat(43).into()]).await.unwrap();
    execute(
        &db.db,
        "UPDATE relay_chat_jobs SET lease_until=0 WHERE reply=$1",
        vec![job.reply.clone().into()],
    )
    .await
    .unwrap();
    let (next, _) = executor(&db).await;
    assert!(
        reserve(&db, &next).await.is_none(),
        "an executing action is never retried automatically"
    );
    let state = |db: PgStore, reply: String| async move {
        query(&db.db,"SELECT j.state,m.progress->>'state' AS phase FROM relay_chat_jobs j JOIN relay_chat_messages m ON m.id=j.reply WHERE j.reply=$1",vec![reply.into()])
            .await.unwrap().map(|r| (r.try_get::<String>("","state").unwrap(), r.try_get::<Option<String>>("","phase").unwrap()))
    };
    assert_eq!(
        state(db.clone(), job.reply.clone()).await,
        Some(("waiting".into(), Some("attention".into())))
    );
    // The controller finishes the action; the task resumes from its checkpoint.
    db.control_finish(
        &w,
        &proposed.dispatch.unwrap(),
        json!({"message":"Updated"}),
        true,
    )
    .await
    .unwrap();
    assert_eq!(
        state(db.clone(), job.reply.clone()).await.unwrap().0,
        "queued"
    );
    // The next attempt is built from this checkpoint plus the action receipts.
    let checkpoint: String = query(
        &db.db,
        "SELECT text FROM relay_task_checkpoints WHERE reply=$1",
        vec![job.reply.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "text")
    .unwrap();
    assert_eq!(checkpoint, "Updating order one now.");
    let action: String = query(
        &db.db,
        "SELECT state FROM relay_control_actions WHERE job=$1",
        vec![job.reply.into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "state")
    .unwrap();
    assert_eq!(action, "completed");
}

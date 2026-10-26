use super::{
    control_tests::{request, setup},
    tests::*,
    *,
};
use crate::identity::{
    chat::store::Job,
    interactions::model::{Ask, Submission},
};
use serde_json::json;
fn ask(key: &str) -> Ask {
    serde_json::from_value(json!({"key":key,"specification":{"version":1,"kind":"single_choice","title":"Choose a delivery method","options":[{"id":"air","label":"Air"},{"id":"road","label":"Road"}]}})).unwrap()
}
fn answer() -> Submission {
    Submission {
        id: uuid::Uuid::new_v4().to_string(),
        revision: 1,
        decision: "answer".into(),
        values: json!({"choices":["road"]}),
    }
}
async fn state(db: &PgStore, job: &Job) -> Value {
    query(
        &db.db,
        "SELECT to_jsonb(j) AS data FROM relay_chat_jobs j WHERE workspace=$1 AND reply=$2",
        vec![job.workspace.clone().into(), job.reply.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "data")
    .unwrap()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn question_releases_worker_and_one_answer_reconstructs_the_original_task() {
    let (db, owner, w, connection, job) = setup("ask").await;
    let question = db.ask_question(&job, &ask("delivery")).await.unwrap();
    let id = question["id"].as_str().unwrap();
    assert_eq!(
        db.ask_question(&job, &ask("delivery")).await.unwrap()["id"],
        question["id"]
    );
    let mut changed = ask("delivery");
    changed.specification.title = "Different meaning".into();
    assert!(db.ask_question(&job, &changed).await.is_err());
    assert!(db.ask_question(&job, &ask("another")).await.is_err());
    assert!(
        db.control_propose(&job, &request(&connection, "premature"))
            .await
            .is_err()
    );
    db.chat_finish(
        &job,
        "Waiting for the delivery method; then prepare the quote.",
        "complete",
    )
    .await
    .unwrap();
    assert_eq!(state(&db, &job).await["state"], "waiting");
    assert_eq!(state(&db, &job).await["lease_until"], 0);
    assert!(db.claim_job().await.unwrap().is_none());
    let page = db.chat_messages(&owner, &w, &job.chat).await.unwrap();
    let message = page["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == job.reply)
        .unwrap();
    assert_eq!(message["progress"]["state"], "input");
    assert_eq!(message["questions"][0]["id"], question["id"]);
    assert!(message["questions"][0].get("values").is_none());
    let input = answer();
    let (a, b) = tokio::join!(
        db.answer_question(&owner, &w, id, &input),
        db.answer_question(&owner, &w, id, &input)
    );
    assert_eq!(a.unwrap(), b.unwrap());
    assert_eq!(state(&db, &job).await["run_cycle"], 1);
    let resumed = db.claim_job().await.unwrap().unwrap();
    assert_eq!(resumed.reply, job.reply);
    assert_ne!(resumed.claim, job.claim);
    let saved: Value =
        serde_json::from_str(&db.task_continuation(&resumed).await.unwrap().unwrap()).unwrap();
    assert_eq!(saved["questions"][0]["state"], "answered");
    assert_eq!(saved["questions"][0]["values"], input.values);
    assert!(
        !db.finish_claimed_job(&job, "Late old reply", "complete")
            .await
            .unwrap()
    );
    let action = db
        .control_propose(&resumed, &request(&connection, "after-answer"))
        .await
        .unwrap();
    assert_eq!(action.action["state"], "pending");
    assert!(
        action.dispatch.is_none(),
        "Ordinary answers must not approve business writes"
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn form_acknowledgment_reconciles_jsonb_key_order_but_never_changed_values() {
    let (db, owner, w, _, job) = setup("ask").await;
    let ask:Ask=serde_json::from_value(json!({"key":"details","specification":{"version":1,"kind":"form","title":"Details","fields":[{"id":"z_number","label":"Quantity","kind":"number","required":true},{"id":"a_label","label":"Label","kind":"text","required":true}]}})).unwrap();
    let question = db.ask_question(&job, &ask).await.unwrap();
    let id = question["id"].as_str().unwrap();
    db.chat_finish(&job, "Need details", "complete")
        .await
        .unwrap();
    let mut input = answer();
    input.values = json!({"z_number":4,"a_label":"Saved"});
    db.answer_question(&owner, &w, id, &input).await.unwrap();
    input.values = db.question(&owner, &w, id).await.unwrap()["values"].clone();
    assert_eq!(
        db.answer_question(&owner, &w, id, &input).await.unwrap()["state"],
        "answered"
    );
    input.values["z_number"] = json!(5);
    assert!(db.answer_question(&owner, &w, id, &input).await.is_err());
    input.id = uuid::Uuid::new_v4().to_string();
    assert!(db.answer_question(&owner, &w, id, &input).await.is_err());
    assert_eq!(state(&db, &job).await["run_cycle"], 1);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn reconstructed_business_arguments_reuse_the_original_receipt_despite_key_order() {
    let (db, _owner, _w, connection, job) = setup("allow").await;
    let first = db
        .control_propose(&job, &request(&connection, "one"))
        .await
        .unwrap();
    assert!(first.dispatch.is_some());
    let mut reordered = request(&connection, "one");
    reordered.arguments = json!({"address":"Reviewed address","order":"one"});
    let retry = db.control_propose(&job, &reordered).await.unwrap();
    assert_eq!(retry.action["id"], first.action["id"]);
    assert!(retry.dispatch.is_none());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn early_answer_and_answer_release_races_are_retained_once() {
    let (db, owner, w, _, job) = setup("ask").await;
    let question = db.ask_question(&job, &ask("early")).await.unwrap();
    let input = answer();
    db.answer_question(&owner, &w, question["id"].as_str().unwrap(), &input)
        .await
        .unwrap();
    assert_eq!(state(&db, &job).await["state"], "running");
    db.chat_finish(&job, "An early answer is already saved.", "complete")
        .await
        .unwrap();
    let resumed = db.claim_job().await.unwrap().unwrap();
    assert_eq!(resumed.reply, job.reply);
    let question = db.ask_question(&resumed, &ask("race")).await.unwrap();
    let input = answer();
    let (finished, submitted) = tokio::join!(
        db.chat_finish(&resumed, "Question checkpoint", "complete"),
        db.answer_question(&owner, &w, question["id"].as_str().unwrap(), &input)
    );
    finished.unwrap();
    submitted.unwrap();
    assert_eq!(state(&db, &job).await["run_cycle"], 2);
    let next = db.claim_job().await.unwrap().unwrap();
    assert!(db.claim_job().await.unwrap().is_none());
    db.chat_finish(&next, "Done after the answer", "complete")
        .await
        .unwrap();
    assert_eq!(state(&db, &job).await["state"], "finished");
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn only_the_requester_can_answer_even_when_an_owner_can_view_the_source() {
    let (db, owner, w, _, job) = setup("ask").await;
    let viewer = user(&db, "question-owner").await;
    join(&db, &owner, &w, &viewer, "User").await;
    db.change(&owner, &w, &viewer.id, Some("Owner"))
        .await
        .unwrap();
    let question = db.ask_question(&job, &ask("authority")).await.unwrap();
    let id = question["id"].as_str().unwrap();
    assert!(db.question(&viewer, &w, id).await.is_err());
    db.channel_members(
        &owner,
        &w,
        &job.chat,
        &[owner.id.clone(), viewer.id.clone()],
    )
    .await
    .unwrap();
    assert_eq!(
        db.question(&viewer, &w, id).await.unwrap()["can_respond"],
        false
    );
    assert!(
        db.answer_question(&viewer, &w, id, &answer())
            .await
            .is_err()
    );
    let other = workspace(&db, &viewer).await;
    assert!(db.question(&viewer, &other, id).await.is_err());
    assert!(
        db.answer_question(&viewer, &other, id, &answer())
            .await
            .is_err()
    );
    assert_eq!(
        db.question(&owner, &w, id).await.unwrap()["can_respond"],
        true
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn saved_submission_is_immutable_and_decline_is_not_a_boolean_answer_or_stop() {
    let (db, owner, w, _, job) = setup("ask").await;
    let question = db.ask_question(&job, &ask("decline")).await.unwrap();
    let id = question["id"].as_str().unwrap();
    db.chat_finish(&job, "Missing facts", "complete")
        .await
        .unwrap();
    let mut input = answer();
    input.revision = 2;
    assert!(db.answer_question(&owner, &w, id, &input).await.is_err());
    input.revision = 1;
    input.decision = "decline".into();
    assert!(db.answer_question(&owner, &w, id, &input).await.is_err());
    input.values = json!({});
    assert_eq!(
        db.answer_question(&owner, &w, id, &input).await.unwrap()["state"],
        "declined"
    );
    assert_eq!(
        db.answer_question(&owner, &w, id, &input).await.unwrap()["state"],
        "declined"
    );
    input.decision = "answer".into();
    input.values = json!({"choices":["road"]});
    assert!(db.answer_question(&owner, &w, id, &input).await.is_err());
    assert_eq!(state(&db, &job).await["state"], "queued");
    let resumed = db.claim_job().await.unwrap().unwrap();
    let saved: Value =
        serde_json::from_str(&db.task_continuation(&resumed).await.unwrap().unwrap()).unwrap();
    assert_eq!(saved["questions"][0]["state"], "declined");
    assert_eq!(saved["questions"][0]["values"], json!({}));
    db.chat_finish(
        &resumed,
        "Required facts remain missing; no work performed.",
        "complete",
    )
    .await
    .unwrap();
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn stop_and_access_revocation_disable_pending_questions_without_regrant_revival() {
    let (db, owner, w, _, job) = setup("ask").await;
    let question = db.ask_question(&job, &ask("stop")).await.unwrap();
    let id = question["id"].as_str().unwrap();
    db.chat_finish(&job, "Waiting for facts", "complete")
        .await
        .unwrap();
    db.cancel_assignment(&owner, &w, &job.chat, &job.reply)
        .await
        .unwrap();
    assert_eq!(
        db.question(&owner, &w, id).await.unwrap()["state"],
        "invalidated"
    );
    assert!(db.answer_question(&owner, &w, id, &answer()).await.is_err());
    assert!(db.claim_job().await.unwrap().is_none());
    db.chat_send(
        &owner,
        &w,
        &job.chat,
        &message("A fresh task", Some(&job.agent)),
    )
    .await
    .unwrap();
    let next = db.claim_job().await.unwrap().unwrap();
    let question = db.ask_question(&next, &ask("revoke")).await.unwrap();
    let id = question["id"].as_str().unwrap();
    db.chat_finish(&next, "Waiting for facts", "complete")
        .await
        .unwrap();
    let viewer = user(&db, "remaining-channel-owner").await;
    join(&db, &owner, &w, &viewer, "User").await;
    db.change(&owner, &w, &viewer.id, Some("Owner"))
        .await
        .unwrap();
    db.channel_members(
        &owner,
        &w,
        &job.chat,
        &[owner.id.clone(), viewer.id.clone()],
    )
    .await
    .unwrap();
    db.channel_members(&viewer, &w, &job.chat, std::slice::from_ref(&viewer.id))
        .await
        .unwrap();
    assert!(db.question(&owner, &w, id).await.is_err());
    db.channel_members(
        &viewer,
        &w,
        &job.chat,
        &[owner.id.clone(), viewer.id.clone()],
    )
    .await
    .unwrap();
    assert_eq!(
        db.question(&owner, &w, id).await.unwrap()["state"],
        "invalidated"
    );
    assert!(db.answer_question(&owner, &w, id, &answer()).await.is_err());
    assert!(db.claim_job().await.unwrap().is_none());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn concurrent_answer_and_stop_always_leave_the_original_task_terminal() {
    let (db, owner, w, _, job) = setup("ask").await;
    let question = db.ask_question(&job, &ask("race-stop")).await.unwrap();
    let id = question["id"].as_str().unwrap();
    let input = answer();
    db.chat_finish(&job, "Waiting", "complete").await.unwrap();
    let (_answer, stop) = tokio::join!(
        db.answer_question(&owner, &w, id, &input),
        db.cancel_assignment(&owner, &w, &job.chat, &job.reply)
    );
    stop.unwrap();
    assert_eq!(state(&db, &job).await["state"], "finished");
    assert!(db.claim_job().await.unwrap().is_none());
    assert!(
        ["answered", "invalidated"].contains(
            &db.question(&owner, &w, id).await.unwrap()["state"]
                .as_str()
                .unwrap()
        )
    );
}

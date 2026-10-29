use super::{
    control_tests::{decision, request, setup},
    tests::*,
    *,
};
use crate::identity::chat::store::Job;
use serde_json::json;

async fn state(db: &PgStore, job: &Job) -> Value {
    query(
        &db.db,
        "SELECT to_jsonb(j) AS data FROM relay_chat_jobs j WHERE reply=$1",
        vec![job.reply.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "data")
    .unwrap()
}
async fn complete_action(db: &PgStore, owner: &User, w: &str, job: &Job, id: &str) {
    let dispatch = db
        .control_decide(owner, w, &job.agent, id, &decision("approve"))
        .await
        .unwrap()
        .dispatch
        .unwrap();
    db.control_ready(w, &dispatch).await.unwrap();
    db.control_finish(w, &dispatch, json!({"address":"Reviewed address"}), true)
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn approval_wait_releases_agent_and_reconstructs_one_original_task_from_receipts() {
    let (db, owner, w, connection, job) = setup("ask").await;
    let proposal = db
        .control_propose(&job, &request(&connection, "one"))
        .await
        .unwrap();
    let id = proposal.action["id"].as_str().unwrap();
    assert!(
        db.finish_claimed_job(
            &job,
            "Waiting; after approval prepare the delivery summary.",
            "complete"
        )
        .await
        .unwrap()
    );
    assert_eq!(state(&db, &job).await["state"], "waiting");
    assert_eq!(state(&db, &job).await["lease_until"], 0);
    assert!(db.claim_job().await.unwrap().is_none());
    let messages = db.chat_messages(&owner, &w, &job.chat).await.unwrap();
    let waiting = messages["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == job.reply)
        .unwrap();
    assert_eq!(waiting["state"], "queued");
    assert_eq!(waiting["progress"]["state"], "approval");
    assert_eq!(waiting["can_cancel"], true);

    db.chat_send(
        &owner,
        &w,
        &job.chat,
        &message("Another task on the same agent", Some(&job.agent)),
    )
    .await
    .unwrap();
    let other = db.claim_job().await.unwrap().unwrap();
    assert_ne!(other.reply, job.reply);
    db.chat_finish(&other, "Other task done", "complete")
        .await
        .unwrap();
    complete_action(&db, &owner, &w, &job, id).await;
    assert_eq!(state(&db, &job).await["state"], "queued");
    assert_eq!(state(&db, &job).await["run_cycle"], 1);
    assert!(
        db.control_decide(&owner, &w, &job.agent, id, &decision("approve"))
            .await
            .unwrap()
            .dispatch
            .is_none()
    );
    let resumed = db.claim_job().await.unwrap().unwrap();
    assert_eq!(resumed.reply, job.reply);
    assert_eq!(resumed.prompt, job.prompt);
    assert_ne!(resumed.claim, job.claim);
    assert!(
        !db.finish_claimed_job(&job, "Stale worker", "complete")
            .await
            .unwrap()
    );
    let data: Value =
        serde_json::from_str(&db.task_continuation(&resumed).await.unwrap().unwrap()).unwrap();
    assert_eq!(data["actions"][0]["state"], "completed");
    assert_eq!(data["actions"][0]["decision"], "approve");
    assert_eq!(
        data["checkpoint"],
        "Waiting; after approval prepare the delivery summary."
    );
    assert!(data["actions"][0].get("dispatch_token").is_none());
    assert!(
        db.control_propose(&resumed, &request(&connection, "one"))
            .await
            .unwrap()
            .dispatch
            .is_none()
    );
    db.chat_finish(&resumed, "Original task summary complete", "complete")
        .await
        .unwrap();
    assert_eq!(state(&db, &job).await["state"], "finished");
    assert!(db.claim_job().await.unwrap().is_none());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn decision_before_checkpoint_and_concurrent_release_are_not_lost() {
    let (db, owner, w, connection, job) = setup("ask").await;
    let action = db
        .control_propose(&job, &request(&connection, "early"))
        .await
        .unwrap()
        .action;
    complete_action(&db, &owner, &w, &job, action["id"].as_str().unwrap()).await;
    assert_eq!(state(&db, &job).await["state"], "running");
    db.chat_finish(&job, "Checkpoint after an early approval", "complete")
        .await
        .unwrap();
    assert_eq!(state(&db, &job).await["state"], "queued");
    let resumed = db.claim_job().await.unwrap().unwrap();
    let action = db
        .control_propose(&resumed, &request(&connection, "race"))
        .await
        .unwrap()
        .action;
    let id = action["id"].as_str().unwrap();
    let reject = decision("reject");
    let (finish, decide) = tokio::join!(
        db.chat_finish(&resumed, "A second wait", "complete"),
        db.control_decide(&owner, &w, &job.agent, id, &reject)
    );
    finish.unwrap();
    assert!(decide.unwrap().dispatch.is_none());
    assert_eq!(state(&db, &job).await["state"], "queued");
    assert_eq!(state(&db, &job).await["run_cycle"], 2);
    let next = db.claim_job().await.unwrap().unwrap();
    assert!(db.claim_job().await.unwrap().is_none());
    db.chat_finish(&next, "Rejection reported, no effect inferred", "complete")
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn all_waited_actions_must_settle_and_uncertain_outcomes_hold_continuation() {
    let (db, owner, w, connection, job) = setup("ask").await;
    let a = db
        .control_propose(&job, &request(&connection, "one"))
        .await
        .unwrap()
        .action;
    let b = db
        .control_propose(&job, &request(&connection, "two"))
        .await
        .unwrap()
        .action;
    db.chat_finish(&job, "Two approvals", "complete")
        .await
        .unwrap();
    complete_action(&db, &owner, &w, &job, a["id"].as_str().unwrap()).await;
    assert_eq!(state(&db, &job).await["state"], "waiting");
    let dispatch = db
        .control_decide(
            &owner,
            &w,
            &job.agent,
            b["id"].as_str().unwrap(),
            &decision("approve"),
        )
        .await
        .unwrap()
        .dispatch
        .unwrap();
    assert_eq!(state(&db, &job).await["state"], "waiting");
    db.control_finish(&w, &dispatch, json!({"message":"Outcome lost"}), false)
        .await
        .unwrap();
    assert_eq!(state(&db, &job).await["state"], "waiting");
    assert!(db.claim_job().await.unwrap().is_none());
    let m = db.chat_messages(&owner, &w, &job.chat).await.unwrap();
    assert_eq!(
        m["messages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["id"] == job.reply)
            .unwrap()["progress"]["state"],
        "attention"
    );
    assert!(
        db.control_decide(
            &owner,
            &w,
            &job.agent,
            b["id"].as_str().unwrap(),
            &decision("approve")
        )
        .await
        .unwrap()
        .dispatch
        .is_none()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn stop_invalidates_pending_action_and_cannot_be_undone_by_a_late_decision() {
    let (db, owner, w, connection, job) = setup("ask").await;
    let action = db
        .control_propose(&job, &request(&connection, "stop"))
        .await
        .unwrap()
        .action;
    db.chat_finish(&job, "Waiting", "complete").await.unwrap();
    db.cancel_assignment(&owner, &w, &job.chat, &job.reply)
        .await
        .unwrap();
    assert_eq!(state(&db, &job).await["state"], "finished");
    assert_eq!(
        super::approval_delivery::action(&db.db, &w, action["id"].as_str().unwrap())
            .await
            .unwrap()["state"],
        "invalidated"
    );
    assert!(
        db.control_decide(
            &owner,
            &w,
            &job.agent,
            action["id"].as_str().unwrap(),
            &decision("approve")
        )
        .await
        .is_err()
    );
    assert!(db.claim_job().await.unwrap().is_none());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn request_changes_resumes_with_a_decision_and_grants_no_operation_permission() {
    let (db, owner, w, connection, job) = setup("ask").await;
    let action = db
        .control_propose(&job, &request(&connection, "change"))
        .await
        .unwrap()
        .action;
    db.chat_finish(&job, "Saved proposal before failure", "failed")
        .await
        .unwrap();
    assert_eq!(state(&db, &job).await["state"], "waiting");
    assert!(
        db.control_decide(
            &owner,
            &w,
            &job.agent,
            action["id"].as_str().unwrap(),
            &decision("request_changes")
        )
        .await
        .unwrap()
        .dispatch
        .is_none()
    );
    let resumed = db.claim_job().await.unwrap().unwrap();
    let data: Value =
        serde_json::from_str(&db.task_continuation(&resumed).await.unwrap().unwrap()).unwrap();
    assert_eq!(data["actions"][0]["decision"], "request_changes");
    assert_eq!(data["actions"][0]["state"], "rejected");
    let changed = db
        .control_propose(&resumed, &request(&connection, "changed proposal"))
        .await
        .unwrap();
    assert_eq!(changed.action["state"], "pending");
    assert!(changed.dispatch.is_none());
    assert_ne!(changed.action["id"], action["id"]);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn revoked_source_access_prevents_reconstruction_and_publication_after_resume() {
    let (db, owner, w, connection, job) = setup("ask").await;
    let action = db
        .control_propose(&job, &request(&connection, "private"))
        .await
        .unwrap()
        .action;
    db.chat_finish(&job, "Private checkpoint", "complete")
        .await
        .unwrap();
    complete_action(&db, &owner, &w, &job, action["id"].as_str().unwrap()).await;
    execute(
        &db.db,
        "DELETE FROM relay_chat_people WHERE workspace=$1 AND chat=$2 AND user_id=$3",
        vec![
            w.clone().into(),
            job.chat.clone().into(),
            owner.id.clone().into(),
        ],
    )
    .await
    .unwrap();
    let resumed = db.claim_job().await.unwrap().unwrap();
    assert!(!db.chat_job_allowed(&resumed).await);
    assert!(db.task_continuation(&resumed).await.is_err());
    db.chat_finish(&resumed, "MUST_NOT_PUBLISH_PRIVATE_CHECKPOINT", "complete")
        .await
        .unwrap();
    let row = query(
        &db.db,
        "SELECT state,content FROM relay_chat_messages WHERE id=$1",
        vec![job.reply.into()],
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(row.try_get::<String>("", "state").unwrap(), "cancelled");
    assert!(
        !row.try_get::<String>("", "content")
            .unwrap()
            .contains("MUST_NOT")
    );
}

#[path = "task_recovery_tests.rs"]
mod recovery;

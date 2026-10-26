use super::{
    control_tests::{decision, request, setup},
    tests::*,
};
use crate::identity::{
    chat::organization_model::Lifecycle,
    interactions::model::{Ask, Submission},
};
use serde_json::json;
fn lifecycle(archived: bool, revision: i64) -> Lifecycle {
    Lifecycle {
        request: uuid::Uuid::new_v4().to_string(),
        revision,
        archived,
    }
}
fn question() -> Ask {
    serde_json::from_value(json!({"key":"delivery","specification":{"version":1,"kind":"single_choice","title":"Choose a delivery method","options":[{"id":"road","label":"Road"}]}})).unwrap()
}
fn answer() -> Submission {
    Submission {
        id: uuid::Uuid::new_v4().to_string(),
        revision: 1,
        decision: "answer".into(),
        values: json!({"choices":["road"]}),
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn archived_questions_and_exact_approvals_resume_the_same_original_task() {
    let (db, owner, w, connection, job) = setup("ask").await;
    let question = db.ask_question(&job, &question()).await.unwrap();
    let id = question["id"].as_str().unwrap();
    db.chat_finish(&job, "Waiting for delivery information", "complete")
        .await
        .unwrap();
    db.channel_lifecycle(&owner, &w, &job.chat, Some(&lifecycle(true, 0)))
        .await
        .unwrap();
    assert!(
        db.question(&owner, &w, id).await.unwrap()["can_respond"]
            .as_bool()
            .unwrap()
    );
    db.answer_question(&owner, &w, id, &answer()).await.unwrap();
    let resumed = db.claim_job().await.unwrap().unwrap();
    assert_eq!(resumed.reply, job.reply);
    let proposal = db
        .control_propose(&resumed, &request(&connection, "same-task"))
        .await
        .unwrap();
    assert!(proposal.dispatch.is_none());
    let action = proposal.action["id"].as_str().unwrap();
    db.chat_finish(&resumed, "Waiting for the exact order approval", "complete")
        .await
        .unwrap();
    assert!(db.claim_job().await.unwrap().is_none());
    let approved = db
        .control_decide(&owner, &w, &job.agent, action, &decision("approve"))
        .await
        .unwrap()
        .dispatch
        .unwrap();
    db.control_ready(&w, &approved).await.unwrap();
    db.control_finish(&w, &approved, json!({"confirmed":"test receipt"}), true)
        .await
        .unwrap();
    let final_job = db.claim_job().await.unwrap().unwrap();
    assert_eq!(final_job.reply, job.reply);
    db.chat_finish(
        &final_job,
        "Original task finished after its separate approval",
        "complete",
    )
    .await
    .unwrap();
    assert_eq!(
        db.channel_lifecycle(&owner, &w, &job.chat, None)
            .await
            .unwrap()["archived"],
        true
    );
    assert!(
        db.response_body(&owner, &w, &job.chat, &job.reply, false)
            .await
            .unwrap()["body"]
            .as_str()
            .unwrap()
            .contains("Original task finished")
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn restore_cannot_revive_stopped_waits_or_make_direct_chats_archivable() {
    use crate::identity::chat::store::NewChat;
    let (db, owner, w, _, job) = setup("ask").await;
    let question = db.ask_question(&job, &question()).await.unwrap();
    let id = question["id"].as_str().unwrap();
    db.chat_finish(&job, "Waiting for input", "complete")
        .await
        .unwrap();
    db.channel_lifecycle(&owner, &w, &job.chat, Some(&lifecycle(true, 0)))
        .await
        .unwrap();
    db.cancel_assignment(&owner, &w, &job.chat, &job.reply)
        .await
        .unwrap();
    db.channel_lifecycle(&owner, &w, &job.chat, Some(&lifecycle(false, 1)))
        .await
        .unwrap();
    assert!(db.answer_question(&owner, &w, id, &answer()).await.is_err());
    assert!(db.claim_job().await.unwrap().is_none());
    let direct = db
        .chat_create(
            &owner,
            &w,
            &NewChat {
                id: uuid::Uuid::new_v4().to_string(),
                name: "Direct agent".into(),
                kind: "direct".into(),
                description: String::new(),
                users: vec![],
                agents: vec![job.agent],
            },
        )
        .await
        .unwrap();
    assert!(
        db.channel_lifecycle(&owner, &w, &direct, Some(&lifecycle(true, 0)))
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn archive_and_intake_races_commit_only_an_accepted_prefix() {
    let db = fixture().await;
    let owner = user(&db, "archive-race-owner").await;
    let w = workspace(&db, &owner).await;
    for _ in 0..8 {
        let c = chat(&db, &owner, &w, vec![], vec![]).await;
        let input = message("Concurrent ordinary message", None);
        let archive = lifecycle(true, 0);
        let (closed, sent) = tokio::join!(
            db.channel_lifecycle(&owner, &w, &c, Some(&archive)),
            db.chat_send(&owner, &w, &c, &input)
        );
        assert_eq!(closed.unwrap()["archived"], true);
        let history = db.chat_messages(&owner, &w, &c).await.unwrap();
        assert_eq!(
            history["messages"].as_array().unwrap().len(),
            usize::from(sent.is_ok())
        );
        assert!(
            db.chat_send(&owner, &w, &c, &message("After archive commits", None))
                .await
                .is_err()
        );
        if sent.is_ok() {
            assert!(db.chat_send(&owner, &w, &c, &input).await.is_ok());
        }
    }
}

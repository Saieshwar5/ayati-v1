use super::{tests::*, *};
use crate::identity::{chat::store::Job, control::broker::invoke};
use serde_json::json;

async fn unavailable(db: &PgStore, job: &Job, ids: &[String], markers: &[&str]) {
    let page = invoke(db, job, "search_conversation_history", json!({"query":""}))
        .await
        .unwrap();
    for marker in markers {
        assert!(!job.prompt.contains(marker));
        assert!(!page.to_string().contains(marker));
    }
    for id in ids {
        let result = invoke(db, job, "read_conversation_history", json!({"message":id}))
            .await
            .unwrap();
        assert_eq!(result["state"], "not_found");
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn history_excludes_library_derived_answers_and_generated_source_titles() {
    use crate::identity::library::model::{AskItem, SavePage, ShareItem};
    let db = fixture().await;
    let owner = user(&db, "history-library-owner").await;
    let w = workspace(&db, &owner).await;
    let (_, a) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    let page = SavePage {
        id: uuid::Uuid::new_v4().to_string(),
        request: uuid::Uuid::new_v4().to_string(),
        revision: 0,
        title: "LIBRARY_PRIVATE_TITLE".into(),
        document: json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"LIBRARY_PRIVATE_CONTENT"}]}]}),
    };
    db.library_save(&owner, &w, &page).await.unwrap();
    let mut share = ShareItem {
        request: uuid::Uuid::new_v4().to_string(),
        revision: 1,
        visibility: "private".into(),
        people: vec![],
        agents: vec![a.clone()],
    };
    db.library_share(&owner, &w, &page.id, &share)
        .await
        .unwrap();
    let ask = AskItem {
        request: uuid::Uuid::new_v4().to_string(),
        revision: 1,
        agent: a.clone(),
        question: "Review the source".into(),
    };
    let receipt = db.library_ask(&owner, &w, &page.id, &ask).await.unwrap();
    let original = db.claim_job().await.unwrap().unwrap();
    db.chat_finish(&original, "LIBRARY_PRIVATE_DERIVED_ANSWER", "complete")
        .await
        .unwrap();
    share.request = uuid::Uuid::new_v4().to_string();
    share.revision = 2;
    share.agents.clear();
    db.library_share(&owner, &w, &page.id, &share)
        .await
        .unwrap();
    db.chat_send(
        &owner,
        &w,
        receipt["chat"].as_str().unwrap(),
        &message("An unrelated new request", Some(&a)),
    )
    .await
    .unwrap();
    let next = db.claim_job().await.unwrap().unwrap();
    unavailable(
        &db,
        &next,
        &[original.reply, ask.request],
        &[
            "LIBRARY_PRIVATE_TITLE",
            "LIBRARY_PRIVATE_DERIVED_ANSWER",
            "LIBRARY_PRIVATE_CONTENT",
        ],
    )
    .await;
    assert!(
        !db.request_identity(&next)
            .await
            .unwrap()
            .to_string()
            .contains("LIBRARY_PRIVATE_TITLE")
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn history_excludes_business_operation_answers_without_reusing_action_authority() {
    let (db, owner, w, connection, task) = super::control_tests::setup("block").await;
    db.control_propose(&task, &super::control_tests::request(&connection, "one"))
        .await
        .unwrap();
    db.chat_finish(&task, "CONTROL_PRIVATE_DERIVED_ANSWER", "complete")
        .await
        .unwrap();
    db.chat_send(
        &owner,
        &w,
        &task.chat,
        &message("Independent history request", Some(&task.agent)),
    )
    .await
    .unwrap();
    let next = db.claim_job().await.unwrap().unwrap();
    unavailable(
        &db,
        &next,
        &[task.reply],
        &["CONTROL_PRIVATE_DERIVED_ANSWER"],
    )
    .await;
    assert!(
        db.control_snapshot(&owner, &w, &task.agent).await.unwrap()["actions"]
            .as_array()
            .unwrap()
            .len()
            == 1
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn history_excludes_event_results_and_handoff_instructions_and_answers() {
    let s = super::event_tests::setup().await;
    s.save().await;
    s.db.accept_event(
        &s.source,
        "history-event",
        &super::event_tests::payload("history-event", "delayed"),
        100000,
    )
    .await
    .unwrap();
    assert_eq!(s.db.dispatch_events().await.unwrap(), 1);
    let event = super::handoff_tests::claim(&s.db, &s.job().await).await;
    s.db.chat_finish(&event, "EVENT_PRIVATE_ANSWER", "complete")
        .await
        .unwrap();
    let next =
        s.db.chat_send(
            &s.owner,
            &s.w,
            &s.c,
            &message("Independent request", Some(&s.a)),
        )
        .await
        .unwrap()
        .unwrap();
    let next = super::handoff_tests::claim(&s.db, &next).await;
    unavailable(
        &s.db,
        &next,
        &[event.reply],
        &["EVENT_PRIVATE_ANSWER", "PRIVATE-ORDER-42"],
    )
    .await;

    let s = super::handoff_tests::setup().await;
    let mut input = super::handoff_tests::request(&s.b, "history-helper");
    input.task = "HANDOFF_PRIVATE_INSTRUCTION".into();
    let receipt = s.db.request_handoff(&s.parent, &input).await.unwrap();
    let child = super::handoff_access::job(&s.db.db, &s.w, receipt["reply"].as_str().unwrap())
        .await
        .unwrap();
    s.db.chat_finish(&s.parent, "Waiting", "complete")
        .await
        .unwrap();
    let child = super::handoff_tests::claim(&s.db, &child).await;
    s.db.chat_finish(&child, "HANDOFF_PRIVATE_ANSWER", "complete")
        .await
        .unwrap();
    let parent = super::handoff_tests::claim(&s.db, &s.parent).await;
    s.db.chat_finish(&parent, "HANDOFF_PARENT_ANSWER", "complete")
        .await
        .unwrap();
    let next =
        s.db.chat_send(
            &s.owner,
            &s.w,
            &s.c,
            &message("Independent request", Some(&s.a)),
        )
        .await
        .unwrap()
        .unwrap();
    let next = super::handoff_tests::claim(&s.db, &next).await;
    unavailable(
        &s.db,
        &next,
        &[parent.reply, child.reply],
        &[
            "HANDOFF_PRIVATE_INSTRUCTION",
            "HANDOFF_PRIVATE_ANSWER",
            "HANDOFF_PARENT_ANSWER",
        ],
    )
    .await;
}

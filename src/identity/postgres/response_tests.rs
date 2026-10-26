use super::{reply_output_tests::begin, tests::*};
use crate::identity::worker_control::model::Completion;
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn completed_library_bodies_are_revoked_in_reader_history_focus_and_replayed_events() {
    use crate::identity::library::model::{AskItem, SavePage, ShareItem};
    let (db, owner, w, _, initial) = super::control_tests::setup("allow").await;
    db.chat_finish(&initial, "Initial task done", "complete")
        .await
        .unwrap();
    let page = SavePage {
        id: uuid::Uuid::new_v4().to_string(),
        request: uuid::Uuid::new_v4().to_string(),
        revision: 0,
        title: "Private source".into(),
        document: serde_json::json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Private reference"}]}]}),
    };
    db.library_save(&owner, &w, &page).await.unwrap();
    let mut sharing = ShareItem {
        request: uuid::Uuid::new_v4().to_string(),
        revision: 1,
        visibility: "private".into(),
        people: vec![],
        agents: vec![initial.agent.clone()],
    };
    db.library_share(&owner, &w, &page.id, &sharing)
        .await
        .unwrap();
    db.library_ask(
        &owner,
        &w,
        &page.id,
        &AskItem {
            request: uuid::Uuid::new_v4().to_string(),
            revision: 1,
            agent: initial.agent,
            question: "Read this source".into(),
        },
    )
    .await
    .unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    db.chat_finish(&job, "PRIVATE_COMPLETED_REPORT", "complete")
        .await
        .unwrap();
    assert_eq!(
        db.response_body(&owner, &w, &job.chat, &job.reply, false)
            .await
            .unwrap()["body"],
        "PRIVATE_COMPLETED_REPORT"
    );
    sharing.request = uuid::Uuid::new_v4().to_string();
    sharing.revision = 2;
    sharing.agents.clear();
    db.library_share(&owner, &w, &page.id, &sharing)
        .await
        .unwrap();
    assert!(
        db.response_body(&owner, &w, &job.chat, &job.reply, false)
            .await
            .is_err()
    );
    assert!(
        db.response_body(&owner, &w, &job.chat, &job.reply, true)
            .await
            .is_err()
    );
    assert!(
        !db.chat_messages(&owner, &w, &job.chat)
            .await
            .unwrap()
            .to_string()
            .contains("PRIVATE_COMPLETED_REPORT")
    );
    assert!(
        db.chat_focus(&owner, &w, &job.chat, &job.reply)
            .await
            .is_err()
    );
    assert!(
        !db.sync_page(&owner, &w, Some(0))
            .await
            .unwrap()
            .to_string()
            .contains("PRIVATE_COMPLETED_REPORT")
    );
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn saved_response_reads_are_scoped_compact_and_preserve_the_exact_final_body() {
    let (db, owner, w, chat, hash, mut update) = begin().await;
    let id = update.assignment.scope.reply.clone();
    update.text = "Draft 🦀\n".repeat(150);
    update.phase = "responding".into();
    db.append_output(&update, &hash).await.unwrap();
    let body = db
        .response_body(&owner, &w, &chat, &id, false)
        .await
        .unwrap();
    assert_eq!(body["body"], update.text);
    assert_eq!(body["state"], "draft");
    let mut history = db.chat_messages(&owner, &w, &chat).await.unwrap();
    assert!(history.to_string().contains("Draft 🦀"));
    crate::response::compact_history(&mut history);
    let preview = history["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == id)
        .unwrap();
    assert_eq!(
        preview["progress"]["text"]
            .as_str()
            .unwrap()
            .chars()
            .count(),
        320
    );
    assert_eq!(preview["response"]["version"], body["version"]);
    assert!(
        db.response_body(&user(&db, "response-stranger").await, &w, &chat, &id, false)
            .await
            .is_err()
    );
    assert!(
        db.response_body(&owner, &w, &uuid::Uuid::new_v4().to_string(), &id, false)
            .await
            .is_err()
    );
    let other = workspace(&db, &owner).await;
    assert!(
        db.response_body(&owner, &other, &chat, &id, false)
            .await
            .is_err()
    );
    let final_text = format!(
        "# Saved report\n\n{}\n\nFINAL_RESULT",
        "A readable row\n".repeat(170)
    );
    db.complete_controlled(
        &Completion {
            assignment: update.assignment,
            state: "complete".into(),
            text: final_text.clone(),
        },
        &hash,
    )
    .await
    .unwrap();
    let saved = db
        .response_body(&owner, &w, &chat, &id, false)
        .await
        .unwrap();
    assert_eq!(saved["body"], final_text);
    assert_eq!(saved["state"], "complete");
    assert_ne!(saved["version"], body["version"]);
    let metadata = db
        .response_body(&owner, &w, &chat, &id, true)
        .await
        .unwrap();
    assert!(metadata.get("body").is_none());
    assert_eq!(metadata["version"], saved["version"]);
    assert!(metadata["sequence"].as_i64() > body["sequence"].as_i64());
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn stopping_retains_ordered_partial_output_without_rewriting_the_cancellation_message() {
    let (db, owner, w, chat, hash, mut update) = begin().await;
    update.phase = "responding".into();
    update.text = "First saved part 🦀\n".into();
    db.append_output(&update, &hash).await.unwrap();
    update.sequence = 2;
    update.text = "Second saved part".into();
    db.append_output(&update, &hash).await.unwrap();
    let id = &update.assignment.scope.reply;
    db.cancel_assignment(&owner, &w, &chat, id).await.unwrap();
    let body = db
        .response_body(&owner, &w, &chat, id, false)
        .await
        .unwrap();
    assert_eq!(body["state"], "interrupted");
    assert_eq!(body["body"], "First saved part 🦀\nSecond saved part");
    let history = db.chat_messages(&owner, &w, &chat).await.unwrap();
    let saved = history["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == *id)
        .unwrap();
    assert_eq!(saved["content"], "Request cancelled.");
    assert_eq!(saved["response"]["version"], body["version"]);
    assert_eq!(saved["response"]["long"], true);
}

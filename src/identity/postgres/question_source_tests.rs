use super::{control_tests::setup, tests::*, *};
use crate::identity::{
    interactions::model::{Ask, Submission},
    library::model::{AskItem, SavePage, ShareItem},
};
use serde_json::json;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn revocation_before_checkpoint_invalidates_the_question_and_regrant_cannot_revive_it() {
    let (db, owner, w, _, job) = setup("ask").await;
    let viewer = user(&db, "early-revocation-owner").await;
    join(&db, &owner, &w, &viewer, "Member").await;
    db.change(&owner, &w, &viewer.id, Some("Admin"), None)
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
    let ask:Ask=serde_json::from_value(json!({"key":"before-release","specification":{"version":1,"kind":"single_choice","title":"Need a fact","options":[{"id":"yes","label":"Yes"}]}})).unwrap();
    let question = db.ask_question(&job, &ask).await.unwrap();
    let id = question["id"].as_str().unwrap();
    db.channel_members(&viewer, &w, &job.chat, std::slice::from_ref(&viewer.id))
        .await
        .unwrap();
    db.channel_members(
        &viewer,
        &w,
        &job.chat,
        &[viewer.id.clone(), owner.id.clone()],
    )
    .await
    .unwrap();
    assert_eq!(
        db.question(&owner, &w, id).await.unwrap()["state"],
        "invalidated"
    );
    assert!(
        !db.finish_claimed_job(&job, "Must not publish after regrant", "complete")
            .await
            .unwrap()
    );
    assert!(db.ask_question(&job, &ask).await.is_err());
    assert!(db.claim_job().await.unwrap().is_none());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn library_revocation_hides_questions_answers_and_replayed_references() {
    let (db, owner, w, _, initial) = setup("ask").await;
    db.chat_finish(&initial, "Initial work complete", "complete")
        .await
        .unwrap();
    let page = SavePage {
        id: uuid::Uuid::new_v4().to_string(),
        request: uuid::Uuid::new_v4().to_string(),
        revision: 0,
        title: "Private reference".into(),
        document: json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Private business reference"}]}]}),
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
            agent: initial.agent.clone(),
            question: "Review this reference".into(),
        },
    )
    .await
    .unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    let ask:Ask=serde_json::from_value(json!({"key":"private-fact","specification":{"version":1,"kind":"form","title":"PRIVATE_LIBRARY_QUESTION","fields":[{"id":"detail","label":"Private detail","kind":"text","required":true}]}})).unwrap();
    let question = db.ask_question(&job, &ask).await.unwrap();
    let id = question["id"].as_str().unwrap();
    db.chat_finish(&job, "Waiting for a private fact", "complete")
        .await
        .unwrap();
    let before = db.sync_page(&owner, &w, None).await.unwrap()["cursor"]
        .as_i64()
        .unwrap();
    assert!(db.question(&owner, &w, id).await.is_ok());
    assert!(
        db.chat_messages(&owner, &w, &job.chat)
            .await
            .unwrap()
            .to_string()
            .contains("PRIVATE_LIBRARY_QUESTION")
    );
    sharing.request = uuid::Uuid::new_v4().to_string();
    sharing.revision = 2;
    sharing.agents.clear();
    db.library_share(&owner, &w, &page.id, &sharing)
        .await
        .unwrap();
    assert!(db.question(&owner, &w, id).await.is_err());
    let input = Submission {
        id: uuid::Uuid::new_v4().to_string(),
        revision: 1,
        decision: "answer".into(),
        values: json!({"detail":"MUST_NOT_RESUME"}),
    };
    assert!(db.answer_question(&owner, &w, id, &input).await.is_err());
    assert!(
        !db.chat_messages(&owner, &w, &job.chat)
            .await
            .unwrap()
            .to_string()
            .contains("PRIVATE_LIBRARY_QUESTION")
    );
    assert!(
        !db.sync_page(&owner, &w, Some(before))
            .await
            .unwrap()
            .to_string()
            .contains("PRIVATE_LIBRARY_QUESTION")
    );
    let state = query(
        &db.db,
        "SELECT state FROM relay_task_questions WHERE workspace=$1 AND id=$2",
        vec![w.into(), id.into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get::<String>("", "state")
    .unwrap();
    assert_eq!(state, "invalidated");
    assert!(db.claim_job().await.unwrap().is_none());
}

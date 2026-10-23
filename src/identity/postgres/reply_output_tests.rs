use super::{
    executor_slot_tests::{context, report, reserve, setup},
    tests::*,
    *,
};
use crate::identity::{
    provider::digest,
    worker_control::{
        model::{AssignedRequest, Completion},
        output::Update,
    },
};

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn source_revocation_redacts_saved_drafts_from_history_and_cursor_replay() {
    use crate::identity::library::model::{AskItem, SavePage, ShareItem};
    let (db, owner, w, _, a, e, slot) = setup().await;
    let page = SavePage {
        id: uuid::Uuid::new_v4().to_string(),
        request: uuid::Uuid::new_v4().to_string(),
        revision: 0,
        title: "Private source".into(),
        document: serde_json::json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Private source evidence"}]}]}),
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
    let asked = db
        .library_ask(
            &owner,
            &w,
            &page.id,
            &AskItem {
                request: uuid::Uuid::new_v4().to_string(),
                revision: 1,
                agent: a,
                question: "Answer privately".into(),
            },
        )
        .await
        .unwrap();
    let chat = asked["chat"].as_str().unwrap();
    assert!(
        db.claim_job().await.unwrap().is_none(),
        "Library requests must honor pooled dispatch"
    );
    report(&db, &e, vec![slot.clone()]).await.unwrap();
    let scope = reserve(&db, &e).await.unwrap();
    let hash = digest(&slot.token);
    let request = AssignedRequest {
        scope: scope.clone(),
        session: uuid::Uuid::new_v4().to_string(),
    };
    db.start_controlled(&request, &hash, &context(&scope))
        .await
        .unwrap();
    let output = Update {
        assignment: request,
        sequence: 1,
        phase: "responding".into(),
        text: "SOURCE_PRIVATE_DRAFT".into(),
    };
    db.append_output(&output, &hash).await.unwrap();
    assert_eq!(
        reply(&db, &owner, &w, chat).await["progress"]["text"],
        "SOURCE_PRIVATE_DRAFT"
    );
    share.request = uuid::Uuid::new_v4().to_string();
    share.revision = 2;
    share.agents.clear();
    db.library_share(&owner, &w, &page.id, &share)
        .await
        .unwrap();
    assert!(db.append_output(&output, &hash).await.is_err());
    let history = db.chat_messages(&owner, &w, chat).await.unwrap();
    assert!(!history.to_string().contains("SOURCE_PRIVATE_DRAFT"));
    assert!(
        !db.sync_page(&owner, &w, Some(0))
            .await
            .unwrap()
            .to_string()
            .contains("SOURCE_PRIVATE_DRAFT")
    );
}

async fn begin() -> (PgStore, User, String, String, String, Update) {
    let (db, owner, w, c, a, e, slot) = setup().await;
    db.chat_send(
        &owner,
        &w,
        &c,
        &message("Stream a bounded answer", Some(&a)),
    )
    .await
    .unwrap();
    report(&db, &e, vec![slot.clone()]).await.unwrap();
    let scope = reserve(&db, &e).await.unwrap();
    let request = AssignedRequest {
        scope: scope.clone(),
        session: uuid::Uuid::new_v4().to_string(),
    };
    let hash = digest(&slot.token);
    db.start_controlled(&request, &hash, &context(&scope))
        .await
        .unwrap();
    let output = Update {
        assignment: request,
        sequence: 1,
        phase: "waiting".into(),
        text: String::new(),
    };
    (db, owner, w, c, hash, output)
}
async fn reply(db: &PgStore, user: &User, w: &str, c: &str) -> Value {
    db.chat_messages(user, w, c).await.unwrap()["messages"][1].clone()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn output_is_ordered_idempotent_and_separate_from_final_content() {
    let (db, owner, w, c, hash, mut update) = begin().await;
    db.append_output(&update, &hash).await.unwrap();
    update.sequence = 2;
    update.phase = "responding".into();
    update.text = "Hello 🦀".into();
    let (a, b) = tokio::join!(
        db.append_output(&update, &hash),
        db.append_output(&update, &hash)
    );
    assert_eq!(a.unwrap(), 2);
    assert_eq!(b.unwrap(), 2);
    let draft = reply(&db, &owner, &w, &c).await;
    assert_eq!(draft["state"], "queued");
    assert_eq!(draft["content"], "");
    assert_eq!(draft["progress"]["text"], "Hello 🦀");
    assert_eq!(draft["can_cancel"], true);
    let revision = draft["revision"].clone();
    db.append_output(&update, &hash).await.unwrap();
    assert_eq!(reply(&db, &owner, &w, &c).await["revision"], revision);
    let mut changed = update.clone();
    changed.text = "Altered replay".into();
    assert!(db.append_output(&changed, &hash).await.is_err());
    changed.sequence = 4;
    assert!(db.append_output(&changed, &hash).await.is_err());
    db.acknowledge(
        &owner,
        &w,
        &c,
        &[update.assignment.scope.reply.clone()],
        true,
    )
    .await
    .unwrap();
    assert!(
        db.receipt_details(&owner, &w, &c, &update.assignment.scope.reply)
            .await
            .is_err()
    );
    let receipts: i64 = query(
        &db.db,
        "SELECT count(*) AS n FROM relay_message_receipts WHERE message=$1",
        vec![update.assignment.scope.reply.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "n")
    .unwrap();
    assert_eq!(receipts, 0);
    let final_text = "Hello 🦀, finished.";
    let completion = Completion {
        assignment: update.assignment.clone(),
        state: "complete".into(),
        text: final_text.into(),
    };
    db.complete_controlled(&completion, &hash).await.unwrap();
    assert_eq!(
        db.complete_controlled(&completion, &hash).await.unwrap(),
        "complete"
    );
    let final_reply = reply(&db, &owner, &w, &c).await;
    assert_eq!(final_reply["content"], final_text);
    assert_eq!(final_reply["state"], "complete");
    assert!(final_reply["progress"].is_null());
    assert!(final_reply["revision"].as_i64() > revision.as_i64());
    assert!(db.append_output(&update, &hash).await.is_err());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn output_checks_scope_session_cancellation_and_current_authority() {
    let (db, owner, w, c, hash, update) = begin().await;
    for dimension in 0..7 {
        let mut wrong = update.clone();
        match dimension {
            0 => wrong.assignment.scope.workspace = uuid::Uuid::new_v4().to_string(),
            1 => wrong.assignment.scope.slot_generation += 1,
            2 => wrong.assignment.scope.generation += 1,
            3 => wrong.assignment.scope.chat = uuid::Uuid::new_v4().to_string(),
            4 => wrong.assignment.scope.reply = uuid::Uuid::new_v4().to_string(),
            5 => wrong.assignment.scope.incarnation = uuid::Uuid::new_v4().to_string(),
            _ => wrong.assignment.session = uuid::Uuid::new_v4().to_string(),
        }
        assert!(db.append_output(&wrong, &hash).await.is_err());
    }
    db.append_output(&update, &hash).await.unwrap();
    db.cancel_assignment(&owner, &w, &c, &update.assignment.scope.reply)
        .await
        .unwrap();
    assert!(db.append_output(&update, &hash).await.is_err());
    let reply = reply(&db, &owner, &w, &c).await;
    assert_eq!(reply["state"], "cancelled");
    assert!(reply["progress"].is_null());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn output_respects_total_size_and_rejects_reasoning_and_invalid_phases() {
    let (db, owner, w, c, hash, mut update) = begin().await;
    let mut invalid = update.clone();
    invalid.phase = "reasoning".into();
    assert!(db.append_output(&invalid, &hash).await.is_err());
    invalid = update.clone();
    invalid.text = "\0".into();
    assert!(db.append_output(&invalid, &hash).await.is_err());
    update.phase = "responding".into();
    update.text = "x".repeat(4096);
    for sequence in 1..=16 {
        update.sequence = sequence;
        db.append_output(&update, &hash).await.unwrap();
    }
    update.sequence = 17;
    update.text = "x".into();
    assert!(db.append_output(&update, &hash).await.is_err());
    assert_eq!(
        reply(&db, &owner, &w, &c).await["progress"]["text"]
            .as_str()
            .unwrap()
            .len(),
        65536
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn expired_output_is_interrupted_and_new_attempt_does_not_append_old_text() {
    let (db, owner, w, c, hash, mut update) = begin().await;
    update.phase = "responding".into();
    update.text = "OLD_ATTEMPT_PRIVATE_DRAFT".into();
    db.append_output(&update, &hash).await.unwrap();
    execute(
        &db.db,
        "UPDATE relay_chat_jobs SET lease_until=0 WHERE claim=$1",
        vec![update.assignment.scope.assignment.clone().into()],
    )
    .await
    .unwrap();
    db.expire_executors(120).await.unwrap();
    assert_eq!(
        reply(&db, &owner, &w, &c).await["progress"]["state"],
        "interrupted"
    );
    assert!(db.append_output(&update, &hash).await.is_err());
    let tx = db.db.begin().await.unwrap();
    execute(&tx,"INSERT INTO relay_worker_assignments(id,workspace,reply,generation,provider,state,lease_seconds) VALUES($1,$2,$3,2,'local','launching',6)",vec![uuid::Uuid::new_v4().to_string().into(),w.clone().into(),update.assignment.scope.reply.clone().into()]).await.unwrap();
    tx.commit().await.unwrap();
    let message = reply(&db, &owner, &w, &c).await;
    assert_eq!(message["progress"]["generation"], 2);
    assert_eq!(message["progress"]["text"], "");
    assert_eq!(message["progress"]["state"], "starting");
}

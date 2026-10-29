use super::{tests::*, *};
use crate::identity::{chat::execution::Prepared, provider::digest, worker_control::model::*};

/// A pooled assignment started from one ready slot; the slot token is the credential.
async fn setup() -> (PgStore, User, String, AssignedRequest, Prepared) {
    let (db, owner, w, c, a, executor, slot) = super::executor_slot_tests::setup().await;
    db.chat_send(&owner, &w, &c, &message("Scoped request", Some(&a)))
        .await
        .unwrap();
    super::executor_slot_tests::report(&db, &executor, vec![slot.clone()])
        .await
        .unwrap();
    let scope = super::executor_slot_tests::reserve(&db, &executor)
        .await
        .unwrap();
    let context = super::executor_slot_tests::context(&scope);
    let request = AssignedRequest {
        scope,
        session: uuid::Uuid::new_v4().to_string(),
    };
    (db, owner, slot.token, request, context)
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn controller_start_replay_keeps_context_and_rejects_a_second_session() {
    let (db, _, token, request, context) = setup().await;
    let hash = digest(&token);
    db.start_controlled(&request, &hash, &context)
        .await
        .unwrap();
    let mut changed = context.clone();
    changed.request = "Changed request".into();
    let replay = db
        .start_controlled(&request, &hash, &changed)
        .await
        .unwrap();
    assert_eq!(replay.request, context.request);
    let mut other = request.clone();
    other.session = uuid::Uuid::new_v4().to_string();
    assert!(db.start_controlled(&other, &hash, &context).await.is_err());
    assert!(db.renew_controlled(&other, &hash).await.is_err());
    assert_eq!(db.renew_controlled(&request, &hash).await.unwrap(), 6);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn controller_completion_receipt_survives_lost_ack_and_rejects_changed_payload() {
    let (db, owner, token, request, context) = setup().await;
    let hash = digest(&token);
    db.start_controlled(&request, &hash, &context)
        .await
        .unwrap();
    let mut completion = Completion {
        assignment: request.clone(),
        state: "complete".into(),
        text: "One final result".into(),
    };
    assert_eq!(
        db.complete_controlled(&completion, &hash).await.unwrap(),
        "complete"
    );
    assert_eq!(
        db.complete_controlled(&completion, &hash).await.unwrap(),
        "complete"
    );
    completion.text = "Changed result".into();
    assert!(db.complete_controlled(&completion, &hash).await.is_err());
    assert!(db.renew_controlled(&request, &hash).await.is_err());
    let history = db
        .chat_messages(&owner, &request.scope.workspace, &request.scope.chat)
        .await
        .unwrap();
    assert_eq!(history["messages"][1]["content"], "One final result");
    assert!(
        db.controlled_assignment(&request.scope, &hash)
            .await
            .unwrap()
            .context
            .is_none()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn controller_capabilities_bind_every_scope_field_and_expire() {
    let (db, _, token, request, context) = setup().await;
    let hash = digest(&token);
    db.start_controlled(&request, &hash, &context)
        .await
        .unwrap();
    let original = serde_json::to_value(&request.scope).unwrap();
    for field in [
        "executor",
        "incarnation",
        "slot",
        "assignment",
        "workspace",
        "agent",
        "chat",
        "reply",
        "thread",
        "generation",
    ] {
        let mut forged = original.clone();
        forged[field] = if field == "generation" {
            serde_json::json!(2)
        } else {
            serde_json::json!(uuid::Uuid::new_v4().to_string())
        };
        let scope: Scope = serde_json::from_value(forged).unwrap();
        assert!(
            db.controlled_assignment(&scope, &hash).await.is_err(),
            "{field}"
        );
    }
    execute(
        &db.db,
        "UPDATE relay_assignment_credentials SET expires=0",
        vec![],
    )
    .await
    .unwrap();
    assert!(
        db.controlled_assignment(&request.scope, &hash)
            .await
            .is_err()
    );
    assert!(db.renew_controlled(&request, &hash).await.is_err());
    assert!(
        db.complete_controlled(
            &Completion {
                assignment: request,
                state: "complete".into(),
                text: "STALE".into()
            },
            &hash
        )
        .await
        .is_err()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn controller_cancellation_revokes_context_and_preserves_terminal_state() {
    let (db, owner, token, request, context) = setup().await;
    let hash = digest(&token);
    db.start_controlled(&request, &hash, &context)
        .await
        .unwrap();
    db.cancel_assignment(
        &owner,
        &request.scope.workspace,
        &request.scope.chat,
        &request.scope.reply,
    )
    .await
    .unwrap();
    assert!(
        db.controlled_assignment(&request.scope, &hash)
            .await
            .is_err()
    );
    assert!(
        db.complete_controlled(
            &Completion {
                assignment: request.clone(),
                state: "complete".into(),
                text: "PRIVATE".into()
            },
            &hash
        )
        .await
        .is_err()
    );
    let history = db
        .chat_messages(&owner, &request.scope.workspace, &request.scope.chat)
        .await
        .unwrap();
    assert_eq!(history["messages"][1]["state"], "cancelled");
    assert!(
        query(
            &db.db,
            "SELECT context IS NULL AS cleared FROM relay_assignment_credentials",
            vec![]
        )
        .await
        .unwrap()
        .unwrap()
        .try_get::<bool>("", "cleared")
        .unwrap()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn controller_final_publication_rechecks_requester_access() {
    let (db, owner, token, request, context) = setup().await;
    let hash = digest(&token);
    db.start_controlled(&request, &hash, &context)
        .await
        .unwrap();
    execute(
        &db.db,
        "DELETE FROM relay_chat_people WHERE workspace=$1 AND chat=$2 AND user_id=$3",
        vec![
            request.scope.workspace.clone().into(),
            request.scope.chat.clone().into(),
            owner.id.into(),
        ],
    )
    .await
    .unwrap();
    assert!(db.renew_controlled(&request, &hash).await.is_err());
    assert_eq!(
        db.complete_controlled(
            &Completion {
                assignment: request.clone(),
                state: "complete".into(),
                text: "PRIVATE".into()
            },
            &hash
        )
        .await
        .unwrap(),
        "cancelled"
    );
    let row = query(
        &db.db,
        "SELECT content,state FROM relay_chat_messages WHERE id=$1",
        vec![request.scope.reply.into()],
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(row.try_get::<String>("", "state").unwrap(), "cancelled");
    assert!(
        !row.try_get::<String>("", "content")
            .unwrap()
            .contains("PRIVATE")
    );
}

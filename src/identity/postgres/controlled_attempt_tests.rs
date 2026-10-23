use super::{tests::*, *};
use crate::identity::{
    chat::execution::Prepared,
    provider::{digest, random_token},
    worker_control::model::*,
};

fn executor() -> Registration {
    Registration {
        pooled: false,
        executor: uuid::Uuid::new_v4().to_string(),
        incarnation: uuid::Uuid::new_v4().to_string(),
        token: random_token(),
    }
}
fn new_reservation() -> Reservation {
    Reservation {
        request: uuid::Uuid::new_v4().to_string(),
        token: random_token(),
    }
}
async fn setup() -> (
    PgStore,
    User,
    Registration,
    Reservation,
    AssignedRequest,
    Prepared,
) {
    let mut db = fixture().await;
    db.assigned = true;
    let owner = user(&db, "control-owner").await;
    let w = workspace(&db, &owner).await;
    let c = chat(&db, &owner, &w, vec![], vec!["agent-one".into()]).await;
    db.chat_send(
        &owner,
        &w,
        &c,
        &message("Scoped request", Some("agent-one")),
    )
    .await
    .unwrap();
    let executor = executor();
    let reservation = new_reservation();
    db.register_executor(&executor).await.unwrap();
    let scope = db
        .reserve_controlled(
            &executor.executor,
            &executor.incarnation,
            &reservation,
            6,
            2,
        )
        .await
        .unwrap()
        .unwrap();
    let context = Prepared {
        version: 1,
        workspace: w,
        agent: scope.agent.clone(),
        reply: scope.reply.clone(),
        model: "fixture".into(),
        provider: Some("openai".into()),
        instructions: "Trusted instructions".into(),
        request: "Scoped request".into(),
        reference_notes: String::new(),
        sources: vec![],
        run_seconds: 180,
    };
    let request = AssignedRequest {
        scope,
        session: uuid::Uuid::new_v4().to_string(),
    };
    (db, owner, executor, reservation, request, context)
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn controller_reservation_and_registration_replay_are_idempotent() {
    let (db, _, registration, reservation, request, _) = setup().await;
    db.register_executor(&registration).await.unwrap();
    assert!(
        db.executor_allowed(
            &registration.executor,
            &registration.incarnation,
            &digest(&registration.token)
        )
        .await
        .is_ok()
    );
    assert!(
        db.executor_allowed(
            &registration.executor,
            &registration.incarnation,
            &digest(&random_token())
        )
        .await
        .is_err()
    );
    let mut forged = registration.clone();
    forged.token = random_token();
    assert!(db.register_executor(&forged).await.is_err());
    let (a, b) = tokio::join!(
        db.reserve_controlled(
            &registration.executor,
            &registration.incarnation,
            &reservation,
            6,
            2
        ),
        db.reserve_controlled(
            &registration.executor,
            &registration.incarnation,
            &reservation,
            6,
            2
        )
    );
    assert_eq!(a.unwrap().unwrap(), request.scope);
    assert_eq!(b.unwrap().unwrap(), request.scope);
    let mut changed = Reservation {
        request: reservation.request.clone(),
        token: random_token(),
    };
    assert!(
        db.reserve_controlled(
            &registration.executor,
            &registration.incarnation,
            &changed,
            6,
            2
        )
        .await
        .is_err()
    );
    changed.request = uuid::Uuid::new_v4().to_string();
    let other = executor();
    db.register_executor(&other).await.unwrap();
    assert!(
        db.release_controlled(&other.executor, &other.incarnation, &request.scope, "lost")
            .await
            .is_err()
    );
    assert_eq!(
        query(
            &db.db,
            "SELECT count(*) AS n FROM relay_worker_assignments",
            vec![]
        )
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "n")
        .unwrap(),
        1
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn controller_start_replay_keeps_context_and_rejects_a_second_session() {
    let (db, _, _, reservation, request, context) = setup().await;
    let hash = digest(&reservation.token);
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
    assert!(
        db.start_assignment(&request.scope.assignment, "local:forged")
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn controller_completion_receipt_survives_lost_ack_and_rejects_changed_payload() {
    let (db, owner, _, reservation, request, context) = setup().await;
    let hash = digest(&reservation.token);
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
    let (db, _, registration, reservation, request, context) = setup().await;
    let hash = digest(&reservation.token);
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
    db.release_controlled(
        &registration.executor,
        &registration.incarnation,
        &request.scope,
        "lost",
    )
    .await
    .unwrap();
    let replacement = new_reservation();
    let newer = db
        .reserve_controlled(
            &registration.executor,
            &registration.incarnation,
            &replacement,
            6,
            2,
        )
        .await
        .unwrap()
        .unwrap();
    assert_ne!(newer.assignment, request.scope.assignment);
    assert_eq!(newer.generation, 2);
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
    let (db, owner, _, reservation, request, context) = setup().await;
    let hash = digest(&reservation.token);
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
    let (db, owner, _, reservation, request, context) = setup().await;
    let hash = digest(&reservation.token);
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

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn controller_stale_completion_racing_recovery_cannot_publish() {
    for _ in 0..3 {
        let (db, owner, registration, reservation, request, context) = setup().await;
        let hash = digest(&reservation.token);
        db.start_controlled(&request, &hash, &context)
            .await
            .unwrap();
        execute(
            &db.db,
            "UPDATE relay_chat_jobs SET lease_until=0 WHERE reply=$1",
            vec![request.scope.reply.clone().into()],
        )
        .await
        .unwrap();
        let replacement = new_reservation();
        let completion = Completion {
            assignment: request.clone(),
            state: "complete".into(),
            text: "STALE".into(),
        };
        let (saved, reserved) = tokio::time::timeout(Duration::from_secs(10), async {
            tokio::join!(
                db.complete_controlled(&completion, &hash),
                db.reserve_controlled(
                    &registration.executor,
                    &registration.incarnation,
                    &replacement,
                    6,
                    2
                )
            )
        })
        .await
        .expect("Completion/recovery lock order stalled");
        assert!(saved.is_err());
        assert_eq!(reserved.unwrap().unwrap().generation, 2);
        let history = db
            .chat_messages(&owner, &request.scope.workspace, &request.scope.chat)
            .await
            .unwrap();
        assert_eq!(history["messages"][1]["state"], "queued");
        assert_ne!(history["messages"][1]["content"], "STALE");
    }
}

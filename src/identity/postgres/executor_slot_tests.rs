use super::{tests::*, *};
use crate::identity::{
    agents::config::{SaveAgent, Settings},
    chat::execution::Prepared,
    executor::model::{Capacity, Profile, SlotReport},
    provider::{digest, random_token},
    worker_control::model::{AssignedRequest, Completion, Registration, Scope},
};

pub(super) async fn agent(db: &PgStore, owner: &User, w: &str) -> (String, String) {
    let input = SaveAgent {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Pool agent".into(),
        role: "Bounded fixture".into(),
        model: "fixture".into(),
        scope: "workspace".into(),
        revision: 0,
        settings: Settings {
            provider: "openai".into(),
            ..Default::default()
        },
    };
    db.save_agent_profile(owner, w, &input).await.unwrap();
    (
        chat(db, owner, w, vec![], vec![input.id.clone()]).await,
        input.id,
    )
}
pub(super) async fn setup() -> (
    PgStore,
    User,
    String,
    String,
    String,
    Registration,
    SlotReport,
) {
    let mut db = fixture().await;
    db.pooled = true;
    let owner = user(&db, "pool-owner").await;
    let w = workspace(&db, &owner).await;
    let (c, a) = agent(&db, &owner, &w).await;
    let executor = Registration {
        pooled: true,
        executor: uuid::Uuid::new_v4().to_string(),
        incarnation: uuid::Uuid::new_v4().to_string(),
        token: random_token(),
    };
    db.register_executor(&executor).await.unwrap();
    let slot = SlotReport {
        slot: uuid::Uuid::new_v4().to_string(),
        generation: 1,
        profile: Profile {
            provider: "openai".into(),
            model: "fixture".into(),
        },
        state: "ready".into(),
        token: random_token(),
        pid: Some(12345),
        preparation_version: 1,
    };
    (db, owner, w, c, a, executor, slot)
}
pub(super) async fn report(db: &PgStore, e: &Registration, slots: Vec<SlotReport>) -> Result<()> {
    db.report_executor(&e.executor, &e.incarnation, &Capacity { slots })
        .await
}
fn limits() -> PoolLimits {
    PoolLimits {
        capacity: 2,
        workspace: 2,
        model: 2,
        lease: 6,
        chat_reserve: 0,
    }
}
pub(super) async fn reserve(db: &PgStore, e: &Registration) -> Option<Scope> {
    db.reserve_ready_slot(
        &e.executor,
        &e.incarnation,
        limits(),
        &[(e.executor.clone(), e.incarnation.clone())],
    )
    .await
    .unwrap()
}
pub(super) fn context(scope: &Scope) -> Prepared {
    Prepared {
        version: 1,
        workspace: scope.workspace.clone(),
        agent: scope.agent.clone(),
        reply: scope.reply.clone(),
        model: "fixture".into(),
        provider: Some("openai".into()),
        instructions: "Trusted fixture".into(),
        request: "Scoped fixture request".into(),
        reference_notes: String::new(),
        sources: vec![],
        run_seconds: 180,
    }
}
pub(super) async fn finish(db: &PgStore, slot: &SlotReport, scope: Scope) {
    let request = AssignedRequest {
        scope: scope.clone(),
        session: uuid::Uuid::new_v4().to_string(),
    };
    db.start_controlled(&request, &digest(&slot.token), &context(&scope))
        .await
        .unwrap();
    db.complete_controlled(
        &Completion {
            assignment: request,
            state: "complete".into(),
            text: "Final".into(),
        },
        &digest(&slot.token),
    )
    .await
    .unwrap();
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn pool_requires_ready_compatible_capacity_and_preserves_unknown_offers() {
    let (db, owner, w, c, a, e, mut slot) = setup().await;
    db.chat_send(&owner, &w, &c, &message("Pool request", Some(&a)))
        .await
        .unwrap();
    assert!(db.claim_job().await.unwrap().is_none());
    assert!(db.reserve_assignment(6, 2).await.unwrap().is_none());
    slot.state = "preparing".into();
    slot.pid = None;
    report(&db, &e, vec![slot.clone()]).await.unwrap();
    assert!(reserve(&db, &e).await.is_none());
    slot.state = "ready".into();
    slot.pid = Some(12);
    slot.profile.model = "other".into();
    // A generation's profile/credential is immutable once reported.
    assert!(report(&db, &e, vec![slot.clone()]).await.is_err());
    slot.generation = 2;
    report(&db, &e, vec![slot.clone()]).await.unwrap();
    assert!(reserve(&db, &e).await.is_none());
    slot.profile.model = "fixture".into();
    slot.generation = 3;
    slot.token = random_token();
    report(&db, &e, vec![slot.clone()]).await.unwrap();
    let scope = reserve(&db, &e).await.unwrap();
    assert_eq!(scope.slot_generation, 3);
    report(&db, &e, vec![slot.clone()]).await.unwrap(); // Stale ready heartbeat cannot erase offer.
    assert_eq!(
        db.pending_offers(&e.executor, &e.incarnation)
            .await
            .unwrap(),
        vec![scope.clone()]
    );
    assert!(reserve(&db, &e).await.is_none());
    finish(&db, &slot, scope).await;
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn pool_limits_global_workspace_and_model_admission_under_concurrent_dispatch() {
    for dimension in 0..3 {
        let (db, owner, w, c, a, e, slot) = setup().await;
        let (c2, a2) = agent(&db, &owner, &w).await;
        db.chat_send(&owner, &w, &c, &message("First", Some(&a)))
            .await
            .unwrap();
        db.chat_send(&owner, &w, &c2, &message("Second", Some(&a2)))
            .await
            .unwrap();
        let mut second = slot.clone();
        second.slot = uuid::Uuid::new_v4().to_string();
        second.token = random_token();
        report(&db, &e, vec![slot, second]).await.unwrap();
        let mut policy = limits();
        match dimension {
            0 => policy.capacity = 1,
            1 => policy.workspace = 1,
            _ => policy.model = 1,
        }
        let connected = [(e.executor.clone(), e.incarnation.clone())];
        let (a, b) = tokio::join!(
            db.reserve_ready_slot(&e.executor, &e.incarnation, policy, &connected),
            db.reserve_ready_slot(&e.executor, &e.incarnation, policy, &connected)
        );
        assert_eq!(
            [a.unwrap(), b.unwrap()]
                .iter()
                .filter(|v| v.is_some())
                .count(),
            1
        );
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn pool_rotates_workspace_admission_even_with_a_single_available_slot() {
    let (db, owner, w, c, a, e, mut slot) = setup().await;
    let (c2, a2) = agent(&db, &owner, &w).await;
    db.chat_send(&owner, &w, &c, &message("First business", Some(&a)))
        .await
        .unwrap();
    db.chat_send(&owner, &w, &c2, &message("Older queued work", Some(&a2)))
        .await
        .unwrap();
    let other = workspace(&db, &owner).await;
    let (oc, oa) = agent(&db, &owner, &other).await;
    db.chat_send(&owner, &other, &oc, &message("Another business", Some(&oa)))
        .await
        .unwrap();
    report(&db, &e, vec![slot.clone()]).await.unwrap();
    let first = reserve(&db, &e).await.unwrap();
    assert_eq!(first.workspace, w);
    finish(&db, &slot, first).await;
    slot.generation += 1;
    slot.token = random_token();
    report(&db, &e, vec![slot]).await.unwrap();
    assert_eq!(reserve(&db, &e).await.unwrap().workspace, other);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn pool_recycles_only_finished_slots_and_rejects_old_generation_capabilities() {
    let (db, owner, w, c, a, e, mut slot) = setup().await;
    db.chat_send(&owner, &w, &c, &message("First", Some(&a)))
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
    let mut replacement = slot.clone();
    replacement.generation = 2;
    replacement.token = random_token();
    execute(
        &db.db,
        "UPDATE relay_executors SET heartbeat=0 WHERE id=$1",
        vec![e.executor.clone().into()],
    )
    .await
    .unwrap();
    assert!(report(&db, &e, vec![replacement.clone()]).await.is_err());
    // Capacity can wait on an uncertain old lease without losing executor liveness.
    db.expire_executors(6).await.unwrap();
    db.executor_allowed(&e.executor, &e.incarnation, &digest(&e.token))
        .await
        .unwrap();
    let mut wrong = request.clone();
    wrong.scope.slot_generation = 2;
    assert!(db.renew_controlled(&wrong, &hash).await.is_err());
    let complete = Completion {
        assignment: request.clone(),
        state: "complete".into(),
        text: "Exact saved output".into(),
    };
    db.complete_controlled(&complete, &hash).await.unwrap();
    assert_eq!(
        db.complete_controlled(&complete, &hash).await.unwrap(),
        "complete"
    );
    slot = replacement;
    report(&db, &e, vec![slot.clone()]).await.unwrap();
    assert!(db.complete_controlled(&complete, &hash).await.is_err());
    assert!(db.renew_controlled(&request, &hash).await.is_err());
    db.chat_send(&owner, &w, &c, &message("Next", Some(&a)))
        .await
        .unwrap();
    let next = reserve(&db, &e).await.unwrap();
    assert_eq!(next.slot, scope.slot);
    assert_eq!(next.slot_generation, 2);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn pool_drain_keeps_existing_offers_but_never_reopens_ready_capacity() {
    let (db, owner, w, c, a, e, slot) = setup().await;
    db.chat_send(&owner, &w, &c, &message("Offered before drain", Some(&a)))
        .await
        .unwrap();
    report(&db, &e, vec![slot.clone()]).await.unwrap();
    let scope = reserve(&db, &e).await.unwrap();
    db.drain_executor(&e.executor, &e.incarnation)
        .await
        .unwrap();
    assert!(reserve(&db, &e).await.is_none());
    assert_eq!(
        db.pending_offers(&e.executor, &e.incarnation)
            .await
            .unwrap(),
        vec![scope.clone()]
    );
    assert!(db.stop_executor(&e.executor, &e.incarnation).await.is_err());
    finish(&db, &slot, scope).await;
    db.stop_executor(&e.executor, &e.incarnation).await.unwrap();
    assert!(report(&db, &e, vec![slot]).await.is_err());
    assert!(
        db.executor_allowed(&e.executor, &e.incarnation, &digest(&e.token))
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn pool_expiry_and_invalid_readiness_reports_fail_closed() {
    let (db, _, _, _, _, e, mut slot) = setup().await;
    slot.pid = None;
    assert!(report(&db, &e, vec![slot.clone()]).await.is_err());
    slot.pid = Some(123);
    slot.state = "running".into();
    assert!(report(&db, &e, vec![slot.clone()]).await.is_err());
    slot.state = "ready".into();
    report(&db, &e, vec![slot.clone()]).await.unwrap();
    let mut bad = slot.clone();
    bad.token = random_token();
    assert!(report(&db, &e, vec![bad]).await.is_err());
    execute(
        &db.db,
        "UPDATE relay_executors SET heartbeat=0 WHERE id=$1",
        vec![e.executor.clone().into()],
    )
    .await
    .unwrap();
    db.expire_executors(6).await.unwrap();
    assert!(report(&db, &e, vec![slot]).await.is_err());
    assert!(reserve(&db, &e).await.is_none());
}

//! Executor tasks run through the real pool: reservation, start, renewal and
//! completion use the same fenced assignment path as chat replies.
use super::{
    executor_slot_tests::{report, reserve},
    tests::*,
    worker_control::Work,
    *,
};
use crate::identity::{
    chat::execution::Prepared,
    executor::model::{Profile, SlotReport},
    provider::{digest, random_token},
    worker_control::model::{AssignedRequest, Completion, Registration},
};

/// One started task, driven the way a pooled executor drives it.
pub(super) struct Run {
    db: PgStore,
    hash: String,
    request: AssignedRequest,
    pub context: Prepared,
    pub actor: String,
    pub agent: String,
}
impl Run {
    pub fn id(&self) -> &str {
        &self.request.scope.reply
    }
    pub async fn renew(&self) -> bool {
        self.db
            .renew_controlled(&self.request, &self.hash)
            .await
            .is_ok()
    }
    pub async fn finish(&self, text: &str) -> bool {
        self.db
            .complete_controlled(
                &Completion {
                    assignment: self.request.clone(),
                    state: "complete".into(),
                    text: text.into(),
                },
                &self.hash,
            )
            .await
            .is_ok()
    }
    /// The same assignment presented with another executor session.
    pub fn stale(&self) -> Self {
        let mut request = self.request.clone();
        request.session = uuid::Uuid::new_v4().to_string();
        Self {
            db: self.db.clone(),
            hash: self.hash.clone(),
            request,
            context: self.context.clone(),
            actor: self.actor.clone(),
            agent: self.agent.clone(),
        }
    }
}

pub(super) async fn executor(db: &PgStore) -> (Registration, SlotReport) {
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
    report(db, &executor, vec![slot.clone()]).await.unwrap();
    (executor, slot)
}

/// Offer the next work to a fresh one-slot executor and start it if it is a task.
/// Reservation also runs lease recovery, as the dispatcher does.
pub(super) async fn claim(db: &PgStore) -> Option<Run> {
    let (executor, slot) = executor(db).await;
    let scope = reserve(db, &executor).await?;
    let hash = digest(&slot.token);
    let record = db.controlled_assignment(&scope, &hash).await.unwrap();
    let Work::Task(task) = &record.work else {
        panic!("Expected an executor task, not a chat reply");
    };
    let context = db.task_context(task).await.unwrap();
    let request = AssignedRequest {
        scope,
        session: uuid::Uuid::new_v4().to_string(),
    };
    db.start_controlled(&request, &hash, &context).await.ok()?;
    Some(Run {
        db: db.clone(),
        hash,
        request,
        context,
        actor: task.actor.clone(),
        agent: task.agent.clone(),
    })
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn chat_replies_take_a_slot_before_queued_samples() {
    let (mut db, owner, w, chat, agent) = super::rehearsal_tests::setup().await;
    db.pooled = true;
    db.start_rehearsal(&owner, &w, &agent, &super::rehearsal_tests::start())
        .await
        .unwrap();
    db.chat_send(&owner, &w, &chat, &message("Live question", Some(&agent)))
        .await
        .unwrap();
    execute(
        &db.db,
        "UPDATE relay_chat_jobs SET dispatch='pooled' WHERE workspace=$1",
        vec![w.clone().into()],
    )
    .await
    .unwrap();
    let (first, _) = executor(&db).await;
    let scope = reserve(&db, &first).await.unwrap();
    assert!(!scope.chat.is_empty(), "the chat reply is offered first");
    let sample = claim(&db).await.unwrap();
    assert!(sample.context.reply == sample.id() && !sample.context.control_tools);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn samples_share_pool_capacity_and_reserved_chat_capacity_stays_free() {
    let (db, owner, w, _, agent) = super::rehearsal_tests::setup().await;
    db.start_rehearsal(&owner, &w, &agent, &super::rehearsal_tests::start())
        .await
        .unwrap();
    for _ in 0..2 {
        let another = workspace(&db, &owner).await;
        let (_, a) = super::executor_slot_tests::agent(&db, &owner, &another).await;
        let mut input = super::rehearsal_tests::start();
        input.agent_revision = 1;
        db.start_rehearsal(&owner, &another, &a, &input)
            .await
            .unwrap();
    }
    // The test pool admits two runs at a time across every executor.
    let (a, b) = tokio::join!(claim(&db), claim(&db));
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_ne!(a.id(), b.id());
    assert!(claim(&db).await.is_none());
    assert!(a.finish("Completed sample").await);
    let next = claim(&db).await.unwrap();
    assert!(next.id() != a.id() && next.id() != b.id());
    // With one run reserved for chat, background tasks leave that capacity free.
    let (executor, _) = executor(&db).await;
    let reserved = PoolLimits {
        capacity: 3,
        workspace: 3,
        model: 3,
        lease: 6,
        chat_reserve: 1,
    };
    let another = workspace(&db, &owner).await;
    let (_, a2) = super::executor_slot_tests::agent(&db, &owner, &another).await;
    let mut input = super::rehearsal_tests::start();
    input.agent_revision = 1;
    db.start_rehearsal(&owner, &another, &a2, &input)
        .await
        .unwrap();
    let connected = [(executor.executor.clone(), executor.incarnation.clone())];
    assert!(
        db.reserve_ready_slot(
            &executor.executor,
            &executor.incarnation,
            reserved,
            &connected
        )
        .await
        .unwrap()
        .is_none()
    );
}

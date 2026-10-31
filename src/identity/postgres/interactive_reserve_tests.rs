use super::{
    executor_slot_tests::{agent, finish, report, setup},
    tests::*,
    *,
};
use crate::identity::{executor::model::SlotReport, provider::random_token};

fn policy() -> PoolLimits {
    PoolLimits {
        capacity: 4,
        workspace: 4,
        model: 4,
        lease: 6,
        chat_reserve: 1,
    }
}
fn extra(slot: &SlotReport) -> SlotReport {
    SlotReport {
        slot: uuid::Uuid::new_v4().to_string(),
        token: random_token(),
        ..slot.clone()
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn reserve_uses_actual_ready_capacity_and_does_not_parallelize_a_busy_agent() {
    let (db, owner, w, c, a, e, slot) = setup().await;
    let (c2, a2) = agent(&db, &owner, &w).await;
    let (c3, a3) = agent(&db, &owner, &w).await;
    db.chat_background(&owner, &w, &c, &message("Held background", Some(&a)))
        .await
        .unwrap();
    let waiting = db
        .chat_background(&owner, &w, &c2, &message("Queued background", Some(&a2)))
        .await
        .unwrap()
        .unwrap();
    report(&db, &e, vec![slot.clone(), extra(&slot)])
        .await
        .unwrap();
    let peers = [(e.executor.clone(), e.incarnation.clone())];
    let first = db
        .reserve_ready_slot(&e.executor, &e.incarnation, policy(), &peers)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first.agent, a);
    assert!(
        db.reserve_ready_slot(&e.executor, &e.incarnation, policy(), &peers)
            .await
            .unwrap()
            .is_none()
    );
    let busy = db
        .chat_send(&owner, &w, &c, &message("Chat with busy agent", Some(&a)))
        .await
        .unwrap()
        .unwrap();
    let available = db
        .chat_send(
            &owner,
            &w,
            &c3,
            &message("Chat with another agent", Some(&a3)),
        )
        .await
        .unwrap()
        .unwrap();
    let next = db
        .reserve_ready_slot(&e.executor, &e.incarnation, policy(), &peers)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(next.reply, available.reply);
    for reply in [waiting.reply, busy.reply] {
        let row = query(
            &db.db,
            "SELECT attempts,state FROM relay_chat_jobs WHERE reply=$1",
            vec![reply.into()],
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(row.try_get::<i32>("", "attempts").unwrap(), 0);
        assert_eq!(row.try_get::<String>("", "state").unwrap(), "queued");
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn concurrent_background_admission_keeps_global_workspace_and_model_headroom() {
    for dimension in 0..3 {
        let (db, owner, w, c, a, e, slot) = setup().await;
        let (c2, a2) = agent(&db, &owner, &w).await;
        db.chat_background(&owner, &w, &c, &message("First", Some(&a)))
            .await
            .unwrap();
        db.chat_background(&owner, &w, &c2, &message("Second", Some(&a2)))
            .await
            .unwrap();
        report(&db, &e, vec![slot.clone(), extra(&slot), extra(&slot)])
            .await
            .unwrap();
        let mut limits = policy();
        match dimension {
            0 => limits.capacity = 2,
            1 => limits.workspace = 2,
            _ => limits.model = 2,
        }
        let peers = [(e.executor.clone(), e.incarnation.clone())];
        let (one, two) = tokio::join!(
            db.reserve_ready_slot(&e.executor, &e.incarnation, limits, &peers),
            db.reserve_ready_slot(&e.executor, &e.incarnation, limits, &peers)
        );
        assert_eq!(
            [one.unwrap(), two.unwrap()]
                .iter()
                .filter(|x| x.is_some())
                .count(),
            1
        );
        limits.chat_reserve = 2;
        assert!(
            db.reserve_ready_slot(&e.executor, &e.incarnation, limits, &peers)
                .await
                .is_err()
        );
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn disconnected_incompatible_and_preparing_slots_do_not_supply_chat_reserve() {
    let (db, owner, w, c, a, e, slot) = setup().await;
    db.chat_background(&owner, &w, &c, &message("Background", Some(&a)))
        .await
        .unwrap();
    let mut wrong = extra(&slot);
    wrong.profile.model = "other".into();
    let mut preparing = extra(&slot);
    preparing.state = "preparing".into();
    preparing.preparation_version = 0;
    report(&db, &e, vec![slot.clone(), wrong, preparing])
        .await
        .unwrap();
    let mut other = e.clone();
    other.executor = uuid::Uuid::new_v4().to_string();
    other.incarnation = uuid::Uuid::new_v4().to_string();
    other.token = random_token();
    db.register_executor(&other).await.unwrap();
    report(&db, &other, vec![extra(&slot)]).await.unwrap();
    let mut peers = vec![(e.executor.clone(), e.incarnation.clone())];
    assert!(
        db.reserve_ready_slot(&e.executor, &e.incarnation, policy(), &peers)
            .await
            .unwrap()
            .is_none()
    );
    peers.push((other.executor, other.incarnation));
    assert!(
        db.reserve_ready_slot(&e.executor, &e.incarnation, policy(), &peers)
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn durable_turns_allow_background_progress_beside_continuous_chat() {
    let (db, owner, w, c, a, e, slot) = setup().await;
    let bg = db
        .chat_background(&owner, &w, &c, &message("Old background", Some(&a)))
        .await
        .unwrap()
        .unwrap();
    for _ in 0..4 {
        let (chat, agent) = agent(&db, &owner, &w).await;
        db.chat_send(&owner, &w, &chat, &message("Interactive", Some(&agent)))
            .await
            .unwrap();
    }
    let mut fleet = vec![slot.clone(), extra(&slot)];
    report(&db, &e, fleet.clone()).await.unwrap();
    let peers = [(e.executor.clone(), e.incarnation.clone())];
    for turn in 0..4 {
        let scope = db
            .reserve_ready_slot(&e.executor, &e.incarnation, policy(), &peers)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(scope.reply == bg.reply, turn == 3);
        let used = fleet
            .iter_mut()
            .find(|slot| slot.slot == scope.slot)
            .unwrap();
        finish(&db, used, scope).await;
        used.generation += 1;
        used.token = random_token();
        report(&db, &e, fleet.clone()).await.unwrap();
    }
    let row=query(&db.db,"SELECT interactive_streak FROM relay_pool_turns WHERE provider='openai' AND model='fixture'",vec![]).await.unwrap().unwrap();
    assert_eq!(row.try_get::<i16>("", "interactive_streak").unwrap(), 0);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn background_intake_is_authorized_immutable_idempotent_and_separately_bounded() {
    let (db, owner, w, c, a, _, _) = setup().await;
    let stranger = user(&db, "background-stranger").await;
    let input = message("Deferred request", Some(&a));
    assert!(db.chat_background(&stranger, &w, &c, &input).await.is_err());
    let bg = db
        .chat_background(&owner, &w, &c, &input)
        .await
        .unwrap()
        .unwrap();
    assert!(
        db.chat_background(&owner, &w, &c, &input)
            .await
            .unwrap()
            .is_none()
    );
    assert!(db.chat_send(&owner, &w, &c, &input).await.is_err());
    assert!(
        execute(
            &db.db,
            "UPDATE relay_chat_jobs SET work_class='interactive' WHERE reply=$1",
            vec![bg.reply.into()]
        )
        .await
        .is_err()
    );
    for _ in 1..32 {
        db.chat_background(&owner, &w, &c, &message("More deferred", Some(&a)))
            .await
            .unwrap();
    }
    let rejected = message("Too much background", Some(&a));
    assert!(
        db.chat_background(&owner, &w, &c, &rejected)
            .await
            .unwrap_err()
            .is::<crate::identity::chat::model::BackgroundQueueFull>()
    );
    assert!(
        query(
            &db.db,
            "SELECT id FROM relay_chat_messages WHERE id=$1",
            vec![rejected.id.into()]
        )
        .await
        .unwrap()
        .is_none()
    );
    for _ in 0..32 {
        db.chat_send(
            &owner,
            &w,
            &c,
            &message("Reserved queue headroom", Some(&a)),
        )
        .await
        .unwrap();
    }
    assert!(
        db.chat_send(&owner, &w, &c, &message("All full", Some(&a)))
            .await
            .unwrap_err()
            .is::<crate::identity::chat::model::QueueFull>()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn editing_an_agent_cannot_free_its_running_models_reserved_quota() {
    use crate::identity::agents::config::{SaveAgent, Settings};
    let (db, owner, w, c, a, e, slot) = setup().await;
    let (c2, a2) = agent(&db, &owner, &w).await;
    let (c3, a3) = agent(&db, &owner, &w).await;
    db.chat_background(&owner, &w, &c, &message("Running original model", Some(&a)))
        .await
        .unwrap();
    report(&db, &e, vec![slot.clone(), extra(&slot), extra(&slot)])
        .await
        .unwrap();
    let peers = [(e.executor.clone(), e.incarnation.clone())];
    let mut limits = policy();
    limits.model = 2;
    assert!(
        db.reserve_ready_slot(&e.executor, &e.incarnation, limits, &peers)
            .await
            .unwrap()
            .is_some()
    );
    db.save_agent_profile(
        &owner,
        &w,
        &SaveAgent {
            id: a,
            name: "Edited while running".into(),
            role: "Bounded".into(),
            model: "different-model".into(),
            audience: "selected".into(),
            people: None,
            managers: None,
            revision: 1,
            settings: Settings {
                provider: "openai".into(),
                paused: true,
                ..Default::default()
            },
        },
    )
    .await
    .unwrap();
    db.chat_background(
        &owner,
        &w,
        &c2,
        &message("Must leave original model quota", Some(&a2)),
    )
    .await
    .unwrap();
    assert!(
        db.reserve_ready_slot(&e.executor, &e.incarnation, limits, &peers)
            .await
            .unwrap()
            .is_none()
    );
    let chat = db
        .chat_send(
            &owner,
            &w,
            &c3,
            &message("Use reserved original model quota", Some(&a3)),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        db.reserve_ready_slot(&e.executor, &e.incarnation, limits, &peers)
            .await
            .unwrap()
            .unwrap()
            .reply,
        chat.reply
    );
}

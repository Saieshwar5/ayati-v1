use super::{
    executor_slot_tests::{context, report, reserve, setup},
    tests::*,
    *,
};
use crate::identity::{
    agents::config::{SaveAgent, Settings},
    provider::{digest, random_token},
    worker_control::model::{AssignedRequest, Completion, Registration, Reservation},
};
use sea_orm_migration::MigratorTrait;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn unverified_or_unknown_preparation_cannot_supply_new_ready_capacity() {
    let (db, owner, w, c, a, e, mut slot) = setup().await;
    db.chat_send(
        &owner,
        &w,
        &c,
        &message("Wait for a prepared harness", Some(&a)),
    )
    .await
    .unwrap();
    slot.preparation_version = 0;
    report(&db, &e, vec![slot.clone()]).await.unwrap();
    assert!(reserve(&db, &e).await.is_none());
    let row = query(
        &db.db,
        "SELECT state,ready_ms FROM relay_executor_slots WHERE slot=$1",
        vec![slot.slot.clone().into()],
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(row.try_get::<String>("", "state").unwrap(), "preparing");
    assert!(
        row.try_get::<Option<i64>>("", "ready_ms")
            .unwrap()
            .is_none()
    );
    // The database backstop also normalizes an older controller's writes.
    execute(
        &db.db,
        "UPDATE relay_executor_slots SET state='ready' WHERE slot=$1",
        vec![slot.slot.clone().into()],
    )
    .await
    .unwrap();
    let row = query(
        &db.db,
        "SELECT state FROM relay_executor_slots WHERE slot=$1",
        vec![slot.slot.clone().into()],
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(row.try_get::<String>("", "state").unwrap(), "preparing");
    assert!(reserve(&db, &e).await.is_none());
    slot.preparation_version = 2;
    assert!(report(&db, &e, vec![slot.clone()]).await.is_err());
    slot.preparation_version = 1;
    report(&db, &e, vec![slot]).await.unwrap();
    assert!(reserve(&db, &e).await.is_some());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn exact_start_replay_atomically_refreshes_lease_but_denied_replay_does_not() {
    let (db, owner, w, c, a, e, slot) = setup().await;
    db.chat_send(&owner, &w, &c, &message("Replay one start", Some(&a)))
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
    execute(&db.db,"UPDATE relay_chat_jobs SET lease_until=floor(extract(epoch FROM clock_timestamp()))::bigint+2 WHERE claim=$1",vec![scope.assignment.clone().into()]).await.unwrap();
    let mut changed = context(&scope);
    changed.request = "Do not replace saved context".into();
    let replay = db
        .start_controlled(&request, &hash, &changed)
        .await
        .unwrap();
    assert_eq!(replay.request, context(&scope).request);
    let row=query(&db.db,"SELECT lease_until-floor(extract(epoch FROM clock_timestamp()))::bigint AS remaining,attempts FROM relay_chat_jobs WHERE claim=$1",vec![scope.assignment.clone().into()]).await.unwrap().unwrap();
    assert!(row.try_get::<i64>("", "remaining").unwrap() >= 5);
    assert_eq!(row.try_get::<i32>("", "attempts").unwrap(), 1);
    db.save_agent_profile(
        &owner,
        &w,
        &SaveAgent {
            id: a,
            name: "Paused".into(),
            role: "Bounded".into(),
            model: "fixture".into(),
            scope: "workspace".into(),
            revision: 1,
            settings: Settings {
                paused: true,
                provider: "openai".into(),
                ..Default::default()
            },
        },
    )
    .await
    .unwrap();
    let before = query(
        &db.db,
        "SELECT lease_until FROM relay_chat_jobs WHERE claim=$1",
        vec![scope.assignment.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get::<i64>("", "lease_until")
    .unwrap();
    assert!(
        db.start_controlled(&request, &hash, &changed)
            .await
            .is_err()
    );
    let after = query(
        &db.db,
        "SELECT lease_until FROM relay_chat_jobs WHERE claim=$1",
        vec![scope.assignment.into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get::<i64>("", "lease_until")
    .unwrap();
    assert_eq!(before, after);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn v14_upgrade_demotes_unprepared_slots_and_preserves_an_owned_run() {
    let base = std::env::var("RELAY_TEST_DATABASE_URL").unwrap();
    let admin = PgStore::connect(&base).await.unwrap();
    let schema = format!("relay_test_{}", uuid::Uuid::new_v4().simple());
    admin
        .db
        .execute_unprepared(&format!("CREATE SCHEMA {schema}"))
        .await
        .unwrap();
    let mut url = reqwest::Url::parse(&base).unwrap();
    url.query_pairs_mut()
        .append_pair("options", &format!("-c search_path={schema}"));
    let mut old = PgStore::connect(url.as_str()).await.unwrap();
    migration::Migrator::up(&old.db, Some(14)).await.unwrap();
    old.assigned = true;
    let owner = user(&old, "prepare-upgrade-owner").await;
    let w = workspace(&old, &owner).await;
    let a = uuid::Uuid::new_v4().to_string();
    old.save_agent_profile(
        &owner,
        &w,
        &SaveAgent {
            id: a.clone(),
            name: "Upgrade".into(),
            role: "Bounded".into(),
            model: "fixture".into(),
            scope: "workspace".into(),
            revision: 0,
            settings: Settings {
                provider: "openai".into(),
                ..Default::default()
            },
        },
    )
    .await
    .unwrap();
    let c = chat(&old, &owner, &w, vec![], vec![a.clone()]).await;
    old.chat_send(&owner, &w, &c, &message("Preserve ownership", Some(&a)))
        .await
        .unwrap();
    let e = Registration {
        executor: uuid::Uuid::new_v4().to_string(),
        incarnation: uuid::Uuid::new_v4().to_string(),
        token: random_token(),
        pooled: false,
    };
    old.register_executor(&e).await.unwrap();
    let token = random_token();
    let scope = old
        .reserve_controlled(
            &e.executor,
            &e.incarnation,
            &Reservation {
                request: uuid::Uuid::new_v4().to_string(),
                token: token.clone(),
            },
            120,
            2,
        )
        .await
        .unwrap()
        .unwrap();
    let request = AssignedRequest {
        scope: scope.clone(),
        session: uuid::Uuid::new_v4().to_string(),
    };
    old.start_controlled(&request, &digest(&token), &context(&scope))
        .await
        .unwrap();
    let pool = Registration {
        pooled: true,
        executor: uuid::Uuid::new_v4().to_string(),
        incarnation: uuid::Uuid::new_v4().to_string(),
        token: random_token(),
    };
    old.register_executor(&pool).await.unwrap();
    let slot = uuid::Uuid::new_v4().to_string();
    execute(&old.db,"INSERT INTO relay_executor_slots(executor,incarnation,slot,generation,provider,model,state,token_hash,pid,ready_ms) VALUES($1,$2,$3,1,'openai','fixture','ready',$4,12345,1)",vec![pool.executor.into(),pool.incarnation.into(),slot.clone().into(),digest(&random_token()).into()]).await.unwrap();
    old.migrate().await.unwrap();
    old.migrate().await.unwrap();
    old.verify_schema().await.unwrap();
    let row = query(
        &old.db,
        "SELECT state,preparation_version FROM relay_executor_slots WHERE slot=$1",
        vec![slot.into()],
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(row.try_get::<String>("", "state").unwrap(), "preparing");
    assert_eq!(row.try_get::<i32>("", "preparation_version").unwrap(), 0);
    old.db.execute_unprepared(&format!("GRANT USAGE ON SCHEMA {schema} TO relay_app; GRANT SELECT,INSERT,UPDATE,DELETE ON ALL TABLES IN SCHEMA {schema} TO relay_app; GRANT USAGE,SELECT ON ALL SEQUENCES IN SCHEMA {schema} TO relay_app; REVOKE UPDATE,DELETE ON {schema}.relay_audit FROM relay_app; REVOKE INSERT,UPDATE,DELETE ON {schema}.seaql_migrations FROM relay_app;")).await.unwrap();
    url.set_username("relay_app").unwrap();
    let runtime = PgStore::connect(url.as_str()).await.unwrap();
    assert_eq!(
        runtime
            .renew_controlled(&request, &digest(&token))
            .await
            .unwrap(),
        120
    );
    runtime
        .complete_controlled(
            &Completion {
                assignment: request,
                state: "complete".into(),
                text: "Survived preparation migration".into(),
            },
            &digest(&token),
        )
        .await
        .unwrap();
    let row = query(
        &runtime.db,
        "SELECT claim,attempts FROM relay_chat_jobs WHERE reply=$1",
        vec![scope.reply.into()],
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        row.try_get::<String>("", "claim").unwrap(),
        scope.assignment
    );
    assert_eq!(row.try_get::<i32>("", "attempts").unwrap(), 1);
}

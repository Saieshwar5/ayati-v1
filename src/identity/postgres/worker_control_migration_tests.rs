use super::{tests::*, *};
use crate::identity::{
    provider::{digest, random_token},
    worker_control::model::{AssignedRequest, Completion, Scope},
};
use sea_orm_migration::MigratorTrait;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn reply_output_upgrade_preserves_a_reserved_v13_capability() {
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
    let old = PgStore::connect(url.as_str()).await.unwrap();
    migration::Migrator::up(&old.db, Some(13)).await.unwrap();
    let owner = user(&old, "output-upgrade-owner").await;
    let w = workspace(&old, &owner).await;
    let c = chat(&old, &owner, &w, vec![], vec!["agent-one".into()]).await;
    let job = super::migration_fixture::legacy_assigned_request(
        &old,
        &owner,
        &w,
        &c,
        "agent-one",
        "Preserve this pending reply",
    )
    .await
    .unwrap()
    .unwrap();
    let registration = crate::identity::worker_control::model::Registration {
        executor: uuid::Uuid::new_v4().to_string(),
        incarnation: uuid::Uuid::new_v4().to_string(),
        token: random_token(),
        pooled: false,
    };
    super::migration_fixture::legacy_executor(&old, &registration)
        .await
        .unwrap();
    let token = random_token();
    let reservation = super::migration_fixture::LegacyReservation {
        request: uuid::Uuid::new_v4().to_string(),
        token: token.clone(),
    };
    let scope =
        super::migration_fixture::legacy_reservation(&old, &registration, &reservation, 120)
            .await
            .unwrap();
    assert_eq!(scope.reply, job.reply);
    assert_eq!(scope.slot_generation, 0);
    old.migrate().await.unwrap();
    old.migrate().await.unwrap();
    old.verify_schema().await.unwrap();
    old.db.execute_unprepared(&format!("GRANT USAGE ON SCHEMA {schema} TO relay_app; GRANT SELECT,INSERT,UPDATE,DELETE ON ALL TABLES IN SCHEMA {schema} TO relay_app; GRANT USAGE,SELECT ON ALL SEQUENCES IN SCHEMA {schema} TO relay_app; REVOKE UPDATE,DELETE ON {schema}.relay_audit FROM relay_app; REVOKE INSERT,UPDATE,DELETE ON {schema}.seaql_migrations FROM relay_app;")).await.unwrap();
    url.set_username("relay_app").unwrap();
    let runtime = PgStore::connect(url.as_str()).await.unwrap();
    let request = AssignedRequest {
        scope: scope.clone(),
        session: uuid::Uuid::new_v4().to_string(),
    };
    runtime
        .start_controlled(
            &request,
            &digest(&token),
            &super::executor_slot_tests::context(&scope),
        )
        .await
        .unwrap();
    runtime
        .append_output(
            &crate::identity::worker_control::output::Update {
                assignment: request.clone(),
                sequence: 1,
                phase: "responding".into(),
                text: "Preserved live reply".into(),
            },
            &digest(&token),
        )
        .await
        .unwrap();
    let messages = runtime.chat_messages(&owner, &w, &c).await.unwrap();
    assert_eq!(
        messages["messages"][1]["progress"]["text"],
        "Preserved live reply"
    );
    runtime
        .complete_controlled(
            &Completion {
                assignment: request,
                state: "complete".into(),
                text: "Preserved live reply".into(),
            },
            &digest(&token),
        )
        .await
        .unwrap();
    let row = query(
        &runtime.db,
        "SELECT claim,attempts FROM relay_chat_jobs WHERE reply=$1",
        vec![scope.reply.clone().into()],
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

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn executor_slots_migration_preserves_v12_completion_receipts_and_history() {
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
    let old = PgStore::connect(url.as_str()).await.unwrap();
    migration::Migrator::up(&old.db, Some(12)).await.unwrap();
    let owner = user(&old, "slots-upgrade-owner").await;
    let w = workspace(&old, &owner).await;
    let c = chat(&old, &owner, &w, vec![], vec!["agent-one".into()]).await;
    let job = super::migration_fixture::legacy_assigned_request(
        &old,
        &owner,
        &w,
        &c,
        "agent-one",
        "Keep the old receipt",
    )
    .await
    .unwrap()
    .unwrap();
    let scope = Scope {
        executor: uuid::Uuid::new_v4().to_string(),
        incarnation: uuid::Uuid::new_v4().to_string(),
        slot: uuid::Uuid::new_v4().to_string(),
        slot_generation: 0,
        assignment: uuid::Uuid::new_v4().to_string(),
        workspace: w.clone(),
        agent: "agent-one".into(),
        chat: c.clone(),
        thread: None,
        reply: job.reply.clone(),
        generation: 1,
    };
    let completion = Completion {
        assignment: AssignedRequest {
            scope: scope.clone(),
            session: uuid::Uuid::new_v4().to_string(),
        },
        state: "complete".into(),
        text: "Acknowledged before migration".into(),
    };
    let token_hash = digest(&random_token());
    // V12 fingerprints did not contain slot_generation. Seed that exact wire shape.
    let wire = serde_json::to_string(&completion).unwrap();
    assert!(!wire.contains("slot_generation"));
    execute(
        &old.db,
        "UPDATE relay_chat_jobs SET state='finished',claim=$2,attempts=1 WHERE reply=$1",
        vec![job.reply.clone().into(), scope.assignment.clone().into()],
    )
    .await
    .unwrap();
    execute(
        &old.db,
        "UPDATE relay_chat_messages SET state='complete',content=$2 WHERE id=$1",
        vec![job.reply.clone().into(), completion.text.clone().into()],
    )
    .await
    .unwrap();
    execute(&old.db, "INSERT INTO relay_worker_assignments(id,workspace,reply,generation,provider,resource_id,state,outcome,lease_seconds) VALUES($1,$2,$3,1,'local',$4,'finished','complete',120)", vec![scope.assignment.clone().into(),w.clone().into(),job.reply.clone().into(),scope.resource().into()]).await.unwrap();
    execute(&old.db, "INSERT INTO relay_executors(id,incarnation,token_hash,expires) VALUES($1,$2,$3,floor(extract(epoch FROM clock_timestamp()))::bigint+300)", vec![scope.executor.clone().into(),scope.incarnation.clone().into(),digest(&random_token()).into()]).await.unwrap();
    execute(&old.db, "INSERT INTO relay_assignment_credentials(assignment,executor,incarnation,slot,request,token_hash,expires,session,completion_hash,completion_state) VALUES($1,$2,$3,$4,$5,$6,floor(extract(epoch FROM clock_timestamp()))::bigint+300,$7,$8,'complete')", vec![scope.assignment.clone().into(),scope.executor.clone().into(),scope.incarnation.clone().into(),scope.slot.clone().into(),uuid::Uuid::new_v4().to_string().into(),token_hash.clone().into(),completion.assignment.session.clone().into(),digest(&wire).into()]).await.unwrap();
    let history_sql = "SELECT jsonb_agg(to_jsonb(m)-'progress'-'revision'-'edited_at'-'reply_to'-'update_of' ORDER BY sequence) AS data FROM relay_chat_messages m WHERE workspace=$1 AND chat=$2";
    let before: Value = query(
        &old.db,
        history_sql,
        vec![w.clone().into(), c.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "data")
    .unwrap();
    old.migrate().await.unwrap();
    old.migrate().await.unwrap();
    old.verify_schema().await.unwrap();
    old.db.execute_unprepared(&format!("GRANT USAGE ON SCHEMA {schema} TO relay_app; GRANT SELECT,INSERT,UPDATE,DELETE ON ALL TABLES IN SCHEMA {schema} TO relay_app; GRANT USAGE,SELECT ON ALL SEQUENCES IN SCHEMA {schema} TO relay_app; REVOKE UPDATE,DELETE ON {schema}.relay_audit FROM relay_app; REVOKE INSERT,UPDATE,DELETE ON {schema}.seaql_migrations FROM relay_app;")).await.unwrap();
    url.set_username("relay_app").unwrap();
    let runtime = PgStore::connect(url.as_str()).await.unwrap();
    assert_eq!(
        runtime
            .complete_controlled(&completion, &token_hash)
            .await
            .unwrap(),
        "complete"
    );
    let after: Value = query(
        &runtime.db,
        history_sql,
        vec![w.clone().into(), c.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "data")
    .unwrap();
    assert_eq!(after, before);
    assert_eq!(
        runtime.chat_messages(&owner, &w, &c).await.unwrap()["messages"][1]["content"],
        completion.text
    );
    let record = runtime
        .controlled_assignment(&scope, &token_hash)
        .await
        .unwrap();
    assert_eq!(record.scope, scope);
    assert_eq!(
        record.completion_hash.as_deref(),
        Some(digest(&wire).as_str())
    );
    assert!(
        runtime
            .pooled_executor(&scope.executor, &scope.incarnation)
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn worker_control_migration_preserves_a_running_v11_assignment() {
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
    let old = PgStore::connect(url.as_str()).await.unwrap();
    migration::Migrator::up(&old.db, Some(11)).await.unwrap();
    assert!(old.verify_schema().await.is_err());
    let owner = user(&old, "control-upgrade-owner").await;
    let w = workspace(&old, &owner).await;
    let c = chat(&old, &owner, &w, vec![], vec!["agent-one".into()]).await;
    let mut job = super::migration_fixture::legacy_assigned_request(
        &old,
        &owner,
        &w,
        &c,
        "agent-one",
        "Keep this accepted job",
    )
    .await
    .unwrap()
    .unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    execute(&old.db,"UPDATE relay_chat_jobs SET state='running',claim=$2,attempts=1,lease_until=floor(extract(epoch FROM clock_timestamp()))::bigint+120 WHERE reply=$1",
        vec![job.reply.clone().into(),id.clone().into()]).await.unwrap();
    execute(&old.db,"INSERT INTO relay_worker_assignments(id,workspace,reply,generation,provider,resource_id,state,lease_seconds) VALUES($1,$2,$3,1,'local','local:upgrade','running',120)",
        vec![id.clone().into(),w.clone().into(),job.reply.clone().into()]).await.unwrap();
    job.claim = Some(id.clone());
    let payload = query(
        &old.db,
        "SELECT payload FROM relay_chat_jobs WHERE reply=$1",
        vec![job.reply.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get::<Value>("", "payload")
    .unwrap();
    old.migrate().await.unwrap();
    old.migrate().await.unwrap();
    old.verify_schema().await.unwrap();
    let row = query(
        &old.db,
        "SELECT payload,claim,attempts FROM relay_chat_jobs WHERE reply=$1",
        vec![job.reply.clone().into()],
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(row.try_get::<Value>("", "payload").unwrap(), payload);
    assert_eq!(row.try_get::<String>("", "claim").unwrap(), id);
    assert_eq!(row.try_get::<i32>("", "attempts").unwrap(), 1);
    old.db.execute_unprepared(&format!("GRANT USAGE ON SCHEMA {schema} TO relay_app; GRANT SELECT,INSERT,UPDATE,DELETE ON ALL TABLES IN SCHEMA {schema} TO relay_app; GRANT USAGE,SELECT ON ALL SEQUENCES IN SCHEMA {schema} TO relay_app; REVOKE UPDATE,DELETE ON {schema}.relay_audit FROM relay_app; REVOKE INSERT,UPDATE,DELETE ON {schema}.seaql_migrations FROM relay_app;")).await.unwrap();
    url.set_username("relay_app").unwrap();
    let runtime = PgStore::connect(url.as_str()).await.unwrap();
    runtime.verify_schema().await.unwrap();
    assert!(
        runtime
            .finish_claimed_job(&job, "Preserved M1 result", "complete")
            .await
            .unwrap()
    );
    assert_eq!(
        runtime.chat_messages(&owner, &w, &c).await.unwrap()["messages"][1]["content"],
        "Preserved M1 result"
    );
    let executor = crate::identity::worker_control::model::Registration {
        pooled: false,
        executor: uuid::Uuid::new_v4().to_string(),
        incarnation: uuid::Uuid::new_v4().to_string(),
        token: crate::identity::provider::random_token(),
    };
    runtime.register_executor(&executor).await.unwrap();
    assert!(
        runtime
            .executor_allowed(
                &executor.executor,
                &executor.incarnation,
                &crate::identity::provider::digest(&executor.token)
            )
            .await
            .is_ok()
    );
}

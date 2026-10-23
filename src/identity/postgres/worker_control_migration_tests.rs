use super::{tests::*, *};
use sea_orm_migration::MigratorTrait;

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
    let mut old = PgStore::connect(url.as_str()).await.unwrap();
    migration::Migrator::up(&old.db, Some(11)).await.unwrap();
    assert!(old.verify_schema().await.is_err());
    old.assigned = true;
    let owner = user(&old, "control-upgrade-owner").await;
    let w = workspace(&old, &owner).await;
    let c = chat(&old, &owner, &w, vec![], vec!["agent-one".into()]).await;
    let mut job = old
        .chat_send(
            &owner,
            &w,
            &c,
            &message("Keep this accepted job", Some("agent-one")),
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

use super::{tests::*, *};
use sea_orm_migration::MigratorTrait;
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn home_upgrade_preserves_v29_pending_questions_and_continues_the_same_original_task() {
    let url = std::env::var("RELAY_TEST_DATABASE_URL").unwrap();
    let admin = PgStore::connect(&url).await.unwrap();
    let schema = format!("relay_home_upgrade_{}", uuid::Uuid::new_v4().simple());
    admin
        .db
        .execute_unprepared(&format!("CREATE SCHEMA {schema}"))
        .await
        .unwrap();
    let mut scoped = reqwest::Url::parse(&url).unwrap();
    scoped
        .query_pairs_mut()
        .append_pair("options", &format!("-c search_path={schema}"));
    let db = PgStore::connect(scoped.as_str()).await.unwrap();
    migration::Migrator::up(&db.db, Some(29)).await.unwrap();
    assert!(db.verify_schema().await.is_err());
    let owner = user(&db, "home-upgrade-owner").await;
    let w = workspace(&db, &owner).await;
    let (c, a) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    super::migration_fixture::legacy_request(
        &db,
        &owner,
        &w,
        &c,
        &a,
        "Existing task before Home migration",
    )
    .await
    .unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    let q = db
        .ask_question(&job, &super::attention_tests::ask())
        .await
        .unwrap();
    db.chat_finish(&job, "Waiting before schema upgrade.", "complete")
        .await
        .unwrap();
    db.migrate().await.unwrap();
    db.migrate().await.unwrap();
    db.verify_schema().await.unwrap();
    db.answer_question(
        &owner,
        &w,
        q["id"].as_str().unwrap(),
        &crate::identity::interactions::model::Submission {
            id: uuid::Uuid::new_v4().to_string(),
            revision: 1,
            decision: "answer".into(),
            values: serde_json::json!({"choices":["road"]}),
        },
    )
    .await
    .unwrap();
    let resumed = db.claim_job().await.unwrap().unwrap();
    assert_eq!(resumed.reply, job.reply);
    assert!(
        db.task_continuation(&resumed)
            .await
            .unwrap()
            .unwrap()
            .contains("road")
    );
    assert!(
        query(&db.db, "SELECT id FROM relay_home_panels", vec![])
            .await
            .unwrap()
            .is_none()
    );
}

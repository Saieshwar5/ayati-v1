use super::{
    attention_tests::{ask, page},
    tests::*,
    *,
};
use sea_orm_migration::MigratorTrait;
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn attention_upgrade_keeps_v24_pending_questions_and_history_without_starting_work() {
    let base = std::env::var("RELAY_TEST_DATABASE_URL").unwrap();
    let admin = PgStore::connect(&base).await.unwrap();
    let schema = format!("relay_attention_upgrade_{}", uuid::Uuid::new_v4().simple());
    admin
        .db
        .execute_unprepared(&format!("CREATE SCHEMA {schema}"))
        .await
        .unwrap();
    let mut url = reqwest::Url::parse(&base).unwrap();
    url.query_pairs_mut()
        .append_pair("options", &format!("-c search_path={schema}"));
    let db = PgStore::connect(url.as_str()).await.unwrap();
    migration::Migrator::up(&db.db, Some(24)).await.unwrap();
    assert!(db.verify_schema().await.is_err());
    let owner = user(&db, "attention-upgrade-owner").await;
    let w = workspace(&db, &owner).await;
    let (chat, agent) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    super::migration_fixture::legacy_request(
        &db,
        &owner,
        &w,
        &chat,
        &agent,
        "An existing question task",
    )
    .await
    .unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    let question = db.ask_question(&job, &ask()).await.unwrap();
    db.chat_finish(&job, "Existing task is waiting for a fact", "complete")
        .await
        .unwrap();
    db.migrate().await.unwrap();
    db.migrate().await.unwrap();
    db.verify_schema().await.unwrap();
    let attention = db.attention(&owner, &w, &page("needs_you")).await.unwrap();
    assert_eq!(attention["items"].as_array().unwrap().len(), 1);
    assert_eq!(attention["items"][0]["id"], question["id"]);
    assert_eq!(attention["items"][0]["read"], false);
    assert_eq!(
        db.question(&owner, &w, question["id"].as_str().unwrap())
            .await
            .unwrap()["state"],
        "pending"
    );
    assert!(db.claim_job().await.unwrap().is_none());
    assert!(
        db.chat_messages(&owner, &w, &chat)
            .await
            .unwrap()
            .to_string()
            .contains("An existing question task")
    );
}

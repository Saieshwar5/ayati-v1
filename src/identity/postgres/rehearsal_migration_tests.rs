use super::{rehearsal_tests::start, tests::*, *};
use sea_orm_migration::MigratorTrait;
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn rehearsal_upgrade_preserves_v25_questions_attention_and_agent_settings_without_starting_samples()
 {
    let base = std::env::var("RELAY_TEST_DATABASE_URL").unwrap();
    let admin = PgStore::connect(&base).await.unwrap();
    let schema = format!("relay_sample_upgrade_{}", uuid::Uuid::new_v4().simple());
    admin
        .db
        .execute_unprepared(&format!("CREATE SCHEMA {schema}"))
        .await
        .unwrap();
    let mut url = reqwest::Url::parse(&base).unwrap();
    url.query_pairs_mut()
        .append_pair("options", &format!("-c search_path={schema}"));
    let db = PgStore::connect(url.as_str()).await.unwrap();
    migration::Migrator::up(&db.db, Some(25)).await.unwrap();
    assert!(db.verify_schema().await.is_err());
    let owner = user(&db, "sample-upgrade-owner").await;
    let w = workspace(&db, &owner).await;
    let (chat, agent) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    super::migration_fixture::legacy_request(
        &db,
        &owner,
        &w,
        &chat,
        &agent,
        "An existing ordinary request",
    )
    .await
    .unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    let question = db
        .ask_question(&job, &super::attention_tests::ask())
        .await
        .unwrap();
    db.chat_finish(&job, "Waiting for a fact", "complete")
        .await
        .unwrap();
    let attention = db
        .attention(&owner, &w, &crate::identity::attention::Page::default())
        .await
        .unwrap();
    db.acknowledge_attention(
        &owner,
        &w,
        &super::attention_tests::ack(&attention["items"][0], false),
    )
    .await
    .unwrap();
    db.migrate().await.unwrap();
    db.migrate().await.unwrap();
    db.verify_schema().await.unwrap();
    assert_eq!(
        db.attention(&owner, &w, &crate::identity::attention::Page::default())
            .await
            .unwrap()["items"][0]["read"],
        true
    );
    assert_eq!(
        db.question(&owner, &w, question["id"].as_str().unwrap())
            .await
            .unwrap()["state"],
        "pending"
    );
    assert!(db.claim_rehearsal().await.unwrap().is_none());
    assert!(db.claim_job().await.unwrap().is_none());
    let mut input = start();
    input.agent_revision = 1;
    let sample = db
        .start_rehearsal(&owner, &w, &agent, &input)
        .await
        .unwrap();
    assert_eq!(sample["profile"]["revision"], 1);
    assert!(
        sample["profile"]["settings"]
            .get("successful_outcome")
            .is_none()
    );
    assert_eq!(sample["state"], "queued");
}

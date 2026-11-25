use super::{tests::*, *};
use sea_orm_migration::MigratorTrait;
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn event_upgrade_from_v27_registers_no_sources_and_dispatches_nothing() {
    let url = std::env::var("RELAY_TEST_DATABASE_URL").unwrap();
    let admin = PgStore::connect(&url).await.unwrap();
    let schema = format!("relay_event_upgrade_{}", uuid::Uuid::new_v4().simple());
    admin
        .db
        .execute_unprepared(&format!("CREATE SCHEMA {schema}"))
        .await
        .unwrap();
    let mut scoped = reqwest::Url::parse(&url).unwrap();
    scoped
        .query_pairs_mut()
        .append_pair("options", &format!("-c search_path={schema}"));
    let mut db = PgStore::connect(scoped.as_str()).await.unwrap();
    migration::Migrator::up(&db.db, Some(27)).await.unwrap();
    assert!(db.verify_schema().await.is_err());
    db.pooled = true;
    let owner = user(&db, "event-upgrade-owner").await;
    let w = workspace(&db, &owner).await;
    let (_, a) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    db.migrate().await.unwrap();
    db.migrate().await.unwrap();
    db.verify_schema().await.unwrap();
    assert_eq!(
        db.event_triggers(&owner, &w, &a, &[]).await.unwrap()["triggers"],
        serde_json::json!([])
    );
    assert_eq!(db.dispatch_events().await.unwrap(), 0);
    assert_eq!(db.company_counts(&owner, &w).await.unwrap()["jobs"], 0);
}

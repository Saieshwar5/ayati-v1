use super::tests::{chat, fixture, message, user, workspace};
use super::*;
use sea_orm_migration::MigratorTrait;

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn display_names_persist_and_are_used_for_old_messages() {
    let db = fixture().await;
    let owner = user(&db, "owner").await;
    let other = user(&db, "other").await;
    let w = workspace(&db, &owner).await;
    let c = chat(&db, &owner, &w, vec![], vec![]).await;
    db.chat_send(&owner, &w, &c, &message("Hello", None))
        .await
        .unwrap();
    let named = db.update_profile(&owner, "  Sai Eshwar  ").await.unwrap();
    assert_eq!(named.display_name.as_deref(), Some("Sai Eshwar"));
    assert_eq!(
        db.session("owner").await.unwrap().unwrap().0.display_name,
        named.display_name
    );
    assert_eq!(
        db.chat_messages(&owner, &w, &c).await.unwrap()["messages"][0]["name"],
        "Sai Eshwar"
    );
    assert_eq!(
        db.directory(&owner, &w).await.unwrap()["users"][0]["name"],
        "Sai Eshwar"
    );
    assert_eq!(
        db.people(&owner, &w).await.unwrap()["members"][0]["display_name"],
        "Sai Eshwar"
    );
    db.update_profile(&other, "Sai Eshwar").await.unwrap();
    assert!(db.chat_messages(&other, &w, &c).await.is_err());
    for name in ["", " ", "Bad\nName", &"a".repeat(81)] {
        assert!(db.update_profile(&owner, name).await.is_err());
    }
    let again = db
        .login("owner", "new@example.test", "new", "sid")
        .await
        .unwrap();
    assert_eq!(again.id, owner.id);
    assert_eq!(again.display_name, named.display_name);
    db.update_profile(&again, "సాయి").await.unwrap();
    assert_eq!(
        db.chat_messages(&again, &w, &c).await.unwrap()["messages"][0]["name"],
        "సాయి"
    );
    assert_eq!(db.role(&w, &owner.id).await.unwrap(), "Admin");
}

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn display_name_migration_upgrades_existing_accounts() {
    let url = std::env::var("RELAY_TEST_DATABASE_URL").unwrap();
    let admin = PgStore::connect(&url).await.unwrap();
    let schema = format!("relay_test_{}", uuid::Uuid::new_v4().simple());
    admin
        .db
        .execute_unprepared(&format!("CREATE SCHEMA {schema}"))
        .await
        .unwrap();
    let mut url = reqwest::Url::parse(&url).unwrap();
    url.query_pairs_mut()
        .append_pair("options", &format!("-c search_path={schema}"));
    let db = PgStore::connect(url.as_str()).await.unwrap();
    migration::Migrator::up(&db.db, Some(2)).await.unwrap();
    execute(
        &db.db,
        "INSERT INTO relay_users(id,provider,email) VALUES('old','old','old@example.test')",
        vec![],
    )
    .await
    .unwrap();
    db.migrate().await.unwrap();
    db.verify_schema().await.unwrap();
    let old = db
        .login("old", "old@example.test", "a", "sid")
        .await
        .unwrap();
    assert_eq!(old.id, "old");
    assert!(old.display_name.is_none());
    db.update_profile(&old, "Existing person").await.unwrap();
    assert_eq!(
        db.session("a")
            .await
            .unwrap()
            .unwrap()
            .0
            .display_name
            .as_deref(),
        Some("Existing person")
    );
    db.migrate().await.unwrap();
}

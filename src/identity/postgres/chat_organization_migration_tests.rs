use super::{tests::*, *};
use sea_orm_migration::MigratorTrait;
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn organization_upgrade_preserves_populated_v23_history_and_unfinished_work() {
    let base = std::env::var("RELAY_TEST_DATABASE_URL").unwrap();
    let admin = PgStore::connect(&base).await.unwrap();
    let schema = format!("relay_chat_upgrade_{}", uuid::Uuid::new_v4().simple());
    admin
        .db
        .execute_unprepared(&format!("CREATE SCHEMA {schema}"))
        .await
        .unwrap();
    let mut url = reqwest::Url::parse(&base).unwrap();
    url.query_pairs_mut()
        .append_pair("options", &format!("-c search_path={schema}"));
    let db = PgStore::connect(url.as_str()).await.unwrap();
    migration::Migrator::up(&db.db, Some(23)).await.unwrap();
    assert!(db.verify_schema().await.is_err());
    let owner = user(&db, "organization-upgrade-owner").await;
    let w = workspace(&db, &owner).await;
    let (chat, agent) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    let job = super::migration_fixture::legacy_request(
        &db,
        &owner,
        &w,
        &chat,
        &agent,
        "Preserved old request",
    )
    .await
    .unwrap()
    .unwrap();
    db.migrate().await.unwrap();
    db.migrate().await.unwrap();
    db.verify_schema().await.unwrap();
    let conversations = db.chats(&owner, &w).await.unwrap();
    let current = conversations["chats"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == chat)
        .unwrap();
    assert_eq!(current["archived"], false);
    assert_eq!(current["archive_revision"], 0);
    assert_eq!(db.chat_pins(&owner, &w).await.unwrap()["revision"], 0);
    let history = db.chat_messages(&owner, &w, &chat).await.unwrap();
    assert!(history.to_string().contains("Preserved old request"));
    let claimed = db.claim_job().await.unwrap().unwrap();
    assert_eq!(claimed.reply, job.reply);
    assert_eq!(claimed.prompt, job.prompt);
    db.chat_finish(
        &claimed,
        "Old work still finishes after schema upgrade",
        "complete",
    )
    .await
    .unwrap();
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn object_save_rechecks_archive_after_upload_and_pin_reads_do_not_grant_access() {
    use crate::identity::{chat::organization_model::Lifecycle, collaboration::Object};
    let (db, owner, w, _, job) = super::control_tests::setup("ask").await;
    let object = Object {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Stored reference.txt".into(),
        mime: "application/octet-stream".into(),
        size: 10,
        checksum: "0".repeat(64),
    };
    db.check_chat_accepting(&owner, &w, &job.chat)
        .await
        .unwrap();
    db.channel_lifecycle(
        &owner,
        &w,
        &job.chat,
        Some(&Lifecycle {
            request: uuid::Uuid::new_v4().to_string(),
            revision: 0,
            archived: true,
        }),
    )
    .await
    .unwrap();
    assert!(
        db.save_object(&owner, &w, &job.chat, &object)
            .await
            .is_err()
    );
    assert!(db.object(&owner, &w, &object.id).await.is_err());
    let stranger = user(&db, "pin-stranger").await;
    assert!(db.chat_pins(&stranger, &w).await.is_err());
}

//! Unread counts: only active conversations, and only from when a person joined.
use super::super::{
    tests::{chat, fixture, join, message, settings_input, user, workspace},
    *,
};
use crate::identity::chat::organization_model::Lifecycle;
use sea_orm_migration::MigratorTrait;
use serde_json::json;

/// This person's unread count in one conversation (0 when it is not listed).
async fn count(db: &PgStore, person: &User, w: &str, chat: &str) -> i64 {
    db.unread(person, w).await.unwrap()["unread"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["chat"] == chat)
        .map_or(0, |item| item["count"].as_i64().unwrap())
}

/// Moves every join and message so far a minute into the past, so whoever joins
/// next joins after them (message times have one-second resolution).
async fn a_minute_passes(db: &PgStore, w: &str) {
    execute(
        &db.db,
        "UPDATE relay_members SET joined_ms=joined_ms-60000 WHERE workspace=$1",
        vec![w.into()],
    )
    .await
    .unwrap();
    execute(
        &db.db,
        "UPDATE relay_chat_people SET joined_ms=joined_ms-60000 WHERE workspace=$1",
        vec![w.into()],
    )
    .await
    .unwrap();
    execute(
        &db.db,
        "UPDATE relay_chat_messages SET at=at-60 WHERE workspace=$1",
        vec![w.into()],
    )
    .await
    .unwrap();
}

fn lifecycle(archived: bool, revision: i64) -> Lifecycle {
    Lifecycle {
        request: uuid::Uuid::new_v4().to_string(),
        revision,
        archived,
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn unread_counts_only_conversations_chat_lists_as_active() {
    let db = fixture().await;
    let owner = user(&db, "unread-active-owner").await;
    let member = user(&db, "unread-active-member").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &member, "Member").await;
    let c = chat(&db, &owner, &w, vec![member.id.clone()], vec![]).await;
    db.chat_send(&owner, &w, &c, &message("Before archiving", None))
        .await
        .unwrap();
    assert_eq!(count(&db, &member, &w, &c).await, 1);

    db.channel_lifecycle(&owner, &w, &c, Some(&lifecycle(true, 0)))
        .await
        .unwrap();
    assert_eq!(count(&db, &member, &w, &c).await, 0, "archived");
    db.channel_lifecycle(&owner, &w, &c, Some(&lifecycle(false, 1)))
        .await
        .unwrap();
    assert_eq!(count(&db, &member, &w, &c).await, 1, "restored");

    // A schedule's work room never appears in Chat, so its results never count.
    let tx = db.db.begin().await.unwrap();
    let schedule = json!({"id":"daily","agent":"agent-one","principal":member.id,"name":"Daily"});
    let room = super::super::work_rooms::room(&tx, &w, &schedule)
        .await
        .unwrap();
    execute(&tx, "INSERT INTO relay_chat_messages(id,workspace,chat,author,kind,content,state,at) VALUES($1,$2,$3,'agent-one','agent','Run result','complete',$4)",
        vec![uuid::Uuid::new_v4().to_string().into(), w.clone().into(), room.clone().into(), now().into()]).await.unwrap();
    tx.commit().await.unwrap();
    assert_eq!(count(&db, &member, &w, &room).await, 0, "work room");
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_new_member_counts_only_messages_after_joining() {
    let db = fixture().await;
    let owner = user(&db, "unread-join-owner").await;
    let early = user(&db, "unread-join-early").await;
    let late = user(&db, "unread-join-late").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &early, "Member").await;
    let c = chat(&db, &owner, &w, vec![early.id.clone()], vec![]).await;
    db.channel_settings(&owner, &w, &c, &settings_input("public", false, None))
        .await
        .unwrap();
    for text in ["First old message", "Second old message"] {
        db.chat_send(&owner, &w, &c, &message(text, None))
            .await
            .unwrap();
    }
    a_minute_passes(&db, &w).await;

    join(&db, &owner, &w, &late, "Member").await;
    assert_eq!(count(&db, &late, &w, &c).await, 0, "history before joining");
    assert_eq!(count(&db, &early, &w, &c).await, 2);
    db.chat_send(&owner, &w, &c, &message("After joining", None))
        .await
        .unwrap();
    assert_eq!(count(&db, &late, &w, &c).await, 1);
    assert_eq!(count(&db, &early, &w, &c).await, 3);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_person_added_to_a_private_conversation_later_starts_there() {
    let db = fixture().await;
    let owner = user(&db, "unread-added-owner").await;
    let first = user(&db, "unread-added-first").await;
    let added = user(&db, "unread-added-later").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &first, "Member").await;
    join(&db, &owner, &w, &added, "Member").await;
    let c = chat(&db, &owner, &w, vec![first.id.clone()], vec![]).await;
    for text in ["Private one", "Private two"] {
        db.chat_send(&owner, &w, &c, &message(text, None))
            .await
            .unwrap();
    }
    a_minute_passes(&db, &w).await;

    let everyone = [owner.id.clone(), first.id.clone(), added.id.clone()];
    db.channel_members(&owner, &w, &c, &everyone).await.unwrap();
    assert_eq!(count(&db, &added, &w, &c).await, 0, "added after them");
    assert_eq!(
        count(&db, &first, &w, &c).await,
        2,
        "changing members keeps when the others joined"
    );
    db.chat_send(&owner, &w, &c, &message("Welcome", None))
        .await
        .unwrap();
    assert_eq!(count(&db, &added, &w, &c).await, 1);
    assert_eq!(count(&db, &first, &w, &c).await, 3);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn schema_94_starts_existing_members_at_the_upgrade_without_a_rewrite() {
    let url = std::env::var("RELAY_TEST_DATABASE_URL").unwrap();
    let admin = PgStore::connect(&url).await.unwrap();
    let schema = format!("relay_unread_{}", uuid::Uuid::new_v4().simple());
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
    migration::Migrator::up(&db.db, Some(93)).await.unwrap();
    let owner = user(&db, "unread-upgrade-owner").await;
    let member = user(&db, "unread-upgrade-member").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &member, "Member").await;
    let c = chat(&db, &owner, &w, vec![member.id.clone()], vec![]).await;
    db.chat_send(&owner, &w, &c, &message("Sent before the upgrade", None))
        .await
        .unwrap();
    execute(
        &db.db,
        "UPDATE relay_chat_messages SET at=at-60 WHERE workspace=$1",
        vec![w.clone().into()],
    )
    .await
    .unwrap();

    db.migrate().await.unwrap();
    db.verify_schema().await.unwrap();
    // The value for existing rows is stored once in the catalog: no rewrite.
    let stored = query(&db.db, "SELECT bool_and(atthasmissing) AS stored FROM pg_attribute WHERE attname='joined_ms' AND attrelid IN ('relay_members'::regclass,'relay_chat_people'::regclass)", vec![])
        .await.unwrap().unwrap().try_get::<bool>("", "stored").unwrap();
    assert!(stored);
    assert_eq!(
        count(&db, &member, &w, &c).await,
        0,
        "existing members start fresh at the upgrade"
    );
    db.chat_send(&owner, &w, &c, &message("Sent after the upgrade", None))
        .await
        .unwrap();
    assert_eq!(count(&db, &member, &w, &c).await, 1);
    // A person joining after the upgrade gets the time they joined.
    let next = user(&db, "unread-upgrade-next").await;
    join(&db, &owner, &w, &next, "Member").await;
    let joined = query(&db.db, "SELECT (extract(epoch FROM clock_timestamp())*1000)::bigint-joined_ms AS age FROM relay_members WHERE workspace=$1 AND user_id=$2", vec![w.clone().into(), next.id.clone().into()])
        .await.unwrap().unwrap().try_get::<i64>("", "age").unwrap();
    assert!((0..60_000).contains(&joined), "joined {joined} ms ago");
}

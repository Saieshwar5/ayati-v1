//! The Chat list's order: the conversation with the newest activity first.
use super::super::{
    tests::{chat, fixture, join, message, user, workspace},
    *,
};
use sea_orm_migration::MigratorTrait;

/// Conversation IDs in the order this person's Chat list shows them.
async fn order(db: &PgStore, person: &User, w: &str) -> Vec<String> {
    db.chats(person, w).await.unwrap()["chats"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap().to_owned())
        .collect()
}

/// Moves everything so far a minute into the past (message times have one-second
/// resolution), so whatever happens next is newer.
async fn a_minute_passes(db: &PgStore, w: &str) {
    for sql in [
        "UPDATE relay_chats SET created_ms=created_ms-60000 WHERE workspace=$1",
        "UPDATE relay_chat_people SET added_ms=added_ms-60000 WHERE workspace=$1",
        "UPDATE relay_chat_messages SET at=at-60,activity_at=activity_at-60 WHERE workspace=$1",
    ] {
        execute(&db.db, sql, vec![w.into()]).await.unwrap();
    }
}

/// An agent's message straight into a conversation, as the controller writes them.
async fn agent_message(
    db: &PgStore,
    w: &str,
    chat: &str,
    state: &str,
    update_of: Option<&str>,
) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    execute(&db.db, "INSERT INTO relay_chat_messages(id,workspace,chat,author,kind,content,state,at,update_of) VALUES($1,$2,$3,'agent-one','agent','Working on it',$4,$5,$6)",
        vec![id.clone().into(), w.into(), chat.into(), state.into(), now().into(), update_of.map(str::to_owned).into()]).await.unwrap();
    id
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn the_newest_message_puts_its_conversation_first() {
    let db = fixture().await;
    let owner = user(&db, "activity-order-owner").await;
    let w = workspace(&db, &owner).await;
    let mut created = vec![];
    for _ in 0..3 {
        created.push(chat(&db, &owner, &w, vec![], vec![]).await);
        a_minute_passes(&db, &w).await;
    }
    let [a, b, c] = created.try_into().unwrap();
    assert_eq!(
        order(&db, &owner, &w).await,
        [c.as_str(), b.as_str(), a.as_str()],
        "newest created first"
    );

    let in_a = message("In A", None);
    db.chat_send(&owner, &w, &a, &in_a).await.unwrap();
    assert_eq!(
        order(&db, &owner, &w).await,
        [a.as_str(), c.as_str(), b.as_str()]
    );
    a_minute_passes(&db, &w).await;
    let in_b = message("In B", None);
    db.chat_send(&owner, &w, &b, &in_b).await.unwrap();
    a_minute_passes(&db, &w).await;
    db.chat_send(&owner, &w, &c, &message("In C", None))
        .await
        .unwrap();
    a_minute_passes(&db, &w).await;
    assert_eq!(
        order(&db, &owner, &w).await,
        [c.as_str(), b.as_str(), a.as_str()]
    );

    // Progress notes, edits and reactions are not activity.
    agent_message(&db, &w, &b, "sent", Some(&in_b.id)).await;
    db.edit_message(&owner, &w, &b, &in_b.id, "In B, edited")
        .await
        .unwrap();
    db.react(&owner, &w, &b, &in_b.id, "👍", true)
        .await
        .unwrap();
    assert_eq!(
        order(&db, &owner, &w).await,
        [c.as_str(), b.as_str(), a.as_str()]
    );

    // A thread reply is, like any message that counts as unread.
    let mut reply = message("In A's thread", None);
    reply.thread = Some(in_a.id.clone());
    db.chat_send(&owner, &w, &a, &reply).await.unwrap();
    assert_eq!(
        order(&db, &owner, &w).await,
        [a.as_str(), c.as_str(), b.as_str()]
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_reply_that_finishes_later_moves_its_conversation_up() {
    let db = fixture().await;
    let owner = user(&db, "activity-reply-owner").await;
    let w = workspace(&db, &owner).await;
    let a = chat(&db, &owner, &w, vec![], vec![]).await;
    let b = chat(&db, &owner, &w, vec![], vec![]).await;
    let reply = agent_message(&db, &w, &a, "queued", None).await;
    a_minute_passes(&db, &w).await;
    db.chat_send(&owner, &w, &b, &message("Newer", None))
        .await
        .unwrap();
    assert_eq!(
        order(&db, &owner, &w).await,
        [b.as_str(), a.as_str()],
        "a queued reply is not activity"
    );
    a_minute_passes(&db, &w).await;

    execute(
        &db.db,
        "UPDATE relay_chat_messages SET state='complete',content='Done' WHERE id=$1",
        vec![reply.into()],
    )
    .await
    .unwrap();
    assert_eq!(
        order(&db, &owner, &w).await,
        [a.as_str(), b.as_str()],
        "finishing is"
    );
    a_minute_passes(&db, &w).await;
    let failed = agent_message(&db, &w, &b, "queued", None).await;
    execute(
        &db.db,
        "UPDATE relay_chat_messages SET state='failed' WHERE id=$1",
        vec![failed.into()],
    )
    .await
    .unwrap();
    assert_eq!(
        order(&db, &owner, &w).await,
        [a.as_str(), b.as_str()],
        "a failed reply is not"
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn being_added_moves_a_conversation_up_for_that_person_only() {
    let db = fixture().await;
    let owner = user(&db, "activity-added-owner").await;
    let added = user(&db, "activity-added-later").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &added, "Member").await;
    let private = chat(&db, &owner, &w, vec![], vec![]).await;
    db.chat_send(&owner, &w, &private, &message("Before you joined", None))
        .await
        .unwrap();
    a_minute_passes(&db, &w).await;
    let shared = chat(&db, &owner, &w, vec![added.id.clone()], vec![]).await;
    db.chat_send(&owner, &w, &shared, &message("Hello both", None))
        .await
        .unwrap();
    a_minute_passes(&db, &w).await;
    assert_eq!(order(&db, &added, &w).await, [shared.as_str()]);

    db.channel_members(&owner, &w, &private, &[owner.id.clone(), added.id.clone()])
        .await
        .unwrap();
    assert_eq!(
        order(&db, &added, &w).await,
        [private.as_str(), shared.as_str()]
    );
    assert_eq!(
        order(&db, &owner, &w).await,
        [shared.as_str(), private.as_str()],
        "unchanged for the others"
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn schema_96_keeps_the_existing_order_to_messages_without_a_rewrite() {
    let url = std::env::var("RELAY_TEST_DATABASE_URL").unwrap();
    let admin = PgStore::connect(&url).await.unwrap();
    let schema = format!("relay_activity_{}", uuid::Uuid::new_v4().simple());
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
    migration::Migrator::up(&db.db, Some(95)).await.unwrap();
    let owner = user(&db, "activity-upgrade-owner").await;
    let w = workspace(&db, &owner).await;
    let talked = chat(&db, &owner, &w, vec![], vec![]).await;
    let quiet = chat(&db, &owner, &w, vec![], vec![]).await;
    db.chat_send(&owner, &w, &talked, &message("Before the upgrade", None))
        .await
        .unwrap();
    execute(
        &db.db,
        "UPDATE relay_chat_messages SET at=at-600 WHERE workspace=$1",
        vec![w.clone().into()],
    )
    .await
    .unwrap();

    db.migrate().await.unwrap();
    db.verify_schema().await.unwrap();
    // The 0 for existing rows is stored once in the catalog: no table rewrite.
    let stored = query(&db.db, "SELECT bool_and(atthasmissing) AS stored FROM pg_attribute WHERE (attname='created_ms' AND attrelid='relay_chats'::regclass) OR (attname='added_ms' AND attrelid='relay_chat_people'::regclass)", vec![])
        .await.unwrap().unwrap().try_get::<bool>("", "stored").unwrap();
    assert!(stored);
    assert_eq!(
        order(&db, &owner, &w).await,
        [talked.as_str(), quiet.as_str()],
        "the upgrade does not make every conversation new"
    );

    let fresh = chat(&db, &owner, &w, vec![], vec![]).await;
    assert_eq!(
        order(&db, &owner, &w).await,
        [fresh.as_str(), talked.as_str(), quiet.as_str()],
        "a conversation created after it starts at the top"
    );
}

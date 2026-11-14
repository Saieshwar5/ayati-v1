//! Schema 119: bots post into one channel through a secret address, only
//! the channel's managers add or remove them, and their messages ask no one.
use super::chat_bots::Posted;
use super::tests::*;
use super::*;

async fn jobs(db: &PgStore, w: &str) -> i64 {
    query(
        &db.db,
        "SELECT count(*) AS n FROM relay_chat_jobs WHERE workspace=$1",
        vec![w.into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "n")
    .unwrap()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn bots_post_as_themselves_only_with_their_secret_and_ask_no_one() {
    let db = fixture().await;
    let owner = user(&db, "bot-owner").await;
    let w = workspace(&db, &owner).await;
    let member = user(&db, "bot-member").await;
    join(&db, &owner, &w, &member, "Member").await;
    let (c, _agent) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    db.channel_members(&owner, &w, &c, &[owner.id.clone(), member.id.clone()])
        .await
        .unwrap();

    // Only the channel's creator or an admin adds a bot.
    assert!(db.create_bot(&member, &w, &c, "Uptime").await.is_err());
    assert!(db.create_bot(&owner, &w, &c, "  ").await.is_err());
    let (bot, secret) = db
        .create_bot(&owner, &w, &c, " Uptime Monitor ")
        .await
        .unwrap();
    let id = bot["id"].as_str().unwrap().to_owned();
    assert_eq!(bot["name"], "Uptime Monitor");
    let listed = db.chat_bots(&owner, &w, &c).await.unwrap();
    assert_eq!(listed["bots"][0]["id"], id.as_str());
    assert!(
        !listed.to_string().contains(&secret),
        "the secret is shown once only"
    );
    assert!(!listed.to_string().contains("token"));
    assert!(db.chat_bots(&member, &w, &c).await.is_err());

    let before = jobs(&db, &w).await;
    assert_eq!(
        db.bot_post(&id, "wrong", "x").await.unwrap(),
        Posted::Unknown
    );
    assert_eq!(
        db.bot_post(&uuid::Uuid::new_v4().to_string(), &secret, "x")
            .await
            .unwrap(),
        Posted::Unknown
    );
    let Posted::Sent(message) = db
        .bot_post(&id, &secret, "**Payment page is down**")
        .await
        .unwrap()
    else {
        panic!("not sent")
    };
    assert_eq!(jobs(&db, &w).await, before, "a bot never asks an agent");
    let page = db.chat_page(&member, &w, &c, None).await.unwrap();
    let shown = page["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == message.as_str())
        .unwrap()
        .clone();
    assert_eq!(
        (
            shown["kind"].as_str(),
            shown["name"].as_str(),
            shown["author"].as_str()
        ),
        (Some("bot"), Some("Uptime Monitor"), Some(id.as_str()))
    );
    // It counts as unread for people, and search names the bot.
    let unread = db.unread(&member, &w).await.unwrap();
    assert!(
        unread["unread"]
            .as_array()
            .unwrap()
            .iter()
            .any(|u| u["chat"] == c.as_str())
    );
    let found = db
        .search_messages(
            &member,
            &w,
            &super::message_search::SearchInput {
                q: "payment".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(found["results"][0]["name"], "Uptime Monitor");
    let last = db.chat_bots(&owner, &w, &c).await.unwrap()["bots"][0]["last_post"].as_i64();
    assert!(last.is_some());

    // Removing a bot stops its address at once; its messages stay.
    assert!(db.remove_bot(&member, &w, &c, &id).await.is_err());
    db.remove_bot(&owner, &w, &c, &id).await.unwrap();
    assert_eq!(
        db.bot_post(&id, &secret, "again").await.unwrap(),
        Posted::Unknown
    );
    assert!(db.remove_bot(&owner, &w, &c, &id).await.is_err());
    assert_eq!(
        db.chat_bots(&owner, &w, &c).await.unwrap()["bots"],
        serde_json::json!([])
    );
    let page = db.chat_page(&member, &w, &c, None).await.unwrap();
    assert!(
        page["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["id"] == message.as_str() && m["name"] == "Uptime Monitor")
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn bots_are_limited_per_minute_and_stop_in_archived_channels() {
    let db = fixture().await;
    let owner = user(&db, "bot-limit-owner").await;
    let w = workspace(&db, &owner).await;
    let (c, _agent) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    let (bot, secret) = db.create_bot(&owner, &w, &c, "CI").await.unwrap();
    let id = bot["id"].as_str().unwrap().to_owned();
    for _ in 0..30 {
        assert!(matches!(
            db.bot_post(&id, &secret, "build ok").await.unwrap(),
            Posted::Sent(_)
        ));
    }
    assert_eq!(
        db.bot_post(&id, &secret, "one more").await.unwrap(),
        Posted::Limited
    );
    execute(
        &db.db,
        "UPDATE relay_chat_messages SET at=at-61 WHERE workspace=$1 AND author=$2",
        vec![w.clone().into(), id.clone().into()],
    )
    .await
    .unwrap();
    assert!(matches!(
        db.bot_post(&id, &secret, "later").await.unwrap(),
        Posted::Sent(_)
    ));
    execute(
        &db.db,
        "UPDATE relay_chats SET archived=TRUE WHERE workspace=$1 AND id=$2",
        vec![w.clone().into(), c.clone().into()],
    )
    .await
    .unwrap();
    assert_eq!(
        db.bot_post(&id, &secret, "archived").await.unwrap(),
        Posted::Archived
    );
    // Ten bots a channel at most.
    execute(
        &db.db,
        "UPDATE relay_chats SET archived=FALSE WHERE workspace=$1 AND id=$2",
        vec![w.clone().into(), c.clone().into()],
    )
    .await
    .unwrap();
    for n in 0..9 {
        db.create_bot(&owner, &w, &c, &format!("Bot {n}"))
            .await
            .unwrap();
    }
    assert!(db.create_bot(&owner, &w, &c, "Eleventh").await.is_err());
}

//! Telegram against a local fake Bot API: people linking their accounts,
//! who may write, separate rooms, and leaving or unlinking.
use super::super::{tests::*, *};
use super::telegram_fake::*;
use crate::identity::communications::telegram::poller;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn telegram_admits_only_linked_members_who_may_use_the_agent() {
    let db = fixture().await;
    let h = harness().await;
    let (owner, w, nova) = connected(&db, &h, "tg-people-owner", 6001).await;
    let owner_tg = OWNER_TG + 6001;
    let bot = 6001;

    // A stranger who found the bot is refused before any agent work.
    h.fake
        .message(bot, STRANGER_TG, (STRANGER_TG, "private"), Some("Hello"));
    assert_eq!(poller::run_once(&db, &h.settings).await.refused, 1);
    assert!(h.fake.texts(bot, STRANGER_TG)[0].contains("only for members"));
    assert!(db.claim_job().await.unwrap().is_none());

    // A member links once; without access to the agent they are still refused.
    let member = user(&db, "tg-people-member").await;
    join(&db, &owner, &w, &member, "Member").await;
    h.start(
        &db,
        &db.telegram_link_code(&member)
            .await
            .map(|c| link(&c))
            .unwrap(),
        MEMBER_TG,
    )
    .await;
    assert_eq!(
        db.telegram_link_status(&member).await.unwrap()["linked"],
        true
    );
    h.fake
        .message(bot, MEMBER_TG, (MEMBER_TG, "private"), Some("/start"));
    h.fake.message(
        bot,
        MEMBER_TG,
        (MEMBER_TG, "private"),
        Some("What is my leave balance?"),
    );
    assert_eq!(poller::run_once(&db, &h.settings).await.refused, 2);
    assert!(db.claim_job().await.unwrap().is_none());
    db.chat_grant(&owner, &w, &member.id, &nova, true)
        .await
        .unwrap();
    h.fake
        .message(bot, MEMBER_TG, (MEMBER_TG, "private"), Some("/start"));
    h.fake.message(
        bot,
        MEMBER_TG,
        (MEMBER_TG, "private"),
        Some("What is my leave balance?"),
    );
    assert_eq!(poller::run_once(&db, &h.settings).await.admitted, 1);
    assert!(
        h.fake
            .texts(bot, MEMBER_TG)
            .last()
            .unwrap()
            .starts_with("Hi ")
    );
    let first = db.claim_job().await.unwrap().unwrap();
    assert_eq!(first.requester, member.id);

    // The owner gets their own room; reading the same updates again changes nothing.
    h.fake
        .message(bot, owner_tg, (owner_tg, "private"), Some("And mine?"));
    poller::run_once(&db, &h.settings).await;
    let second = db.claim_job().await.unwrap().unwrap();
    assert_eq!(second.requester, owner.id);
    assert_ne!(first.chat, second.chat);
    execute(
        &db.db,
        "UPDATE relay_telegram_bots SET next_update=0 WHERE bot_id=$1",
        vec![bot.into()],
    )
    .await
    .unwrap();
    assert_eq!(poller::run_once(&db, &h.settings).await.admitted, 0);
    assert!(db.claim_job().await.unwrap().is_none());

    // Groups, pictures and other people's accounts.
    h.fake
        .message(bot, MEMBER_TG, (-500, "group"), Some("hi all"));
    h.fake.message(bot, MEMBER_TG, (MEMBER_TG, "private"), None);
    poller::run_once(&db, &h.settings).await;
    assert_eq!(h.fake.calls(bot, "leaveChat")[0]["chat_id"], -500);
    assert!(
        h.fake
            .texts(bot, MEMBER_TG)
            .last()
            .unwrap()
            .contains("only read text")
    );
    let other = user(&db, "tg-people-other").await;
    h.start(
        &db,
        &link(&db.telegram_link_code(&other).await.unwrap()),
        MEMBER_TG,
    )
    .await;
    assert!(
        h.fake
            .texts(MANAGER_ID, MEMBER_TG)
            .last()
            .unwrap()
            .contains("already connected")
    );
    assert_eq!(
        db.telegram_link_status(&other).await.unwrap()["linked"],
        false
    );

    // Leaving the workspace stops it at once; so does unlinking.
    db.change(&owner, &w, &member.id, None, None).await.unwrap();
    h.fake
        .message(bot, MEMBER_TG, (MEMBER_TG, "private"), Some("Still there?"));
    assert_eq!(poller::run_once(&db, &h.settings).await.refused, 1);
    db.telegram_unlink(&owner).await.unwrap();
    h.fake
        .message(bot, owner_tg, (owner_tg, "private"), Some("Me again"));
    assert_eq!(poller::run_once(&db, &h.settings).await.refused, 1);
    assert!(db.claim_job().await.unwrap().is_none());
    // An unlinked account still belongs to its person.
    h.start(
        &db,
        &link(&db.telegram_link_code(&other).await.unwrap()),
        owner_tg,
    )
    .await;
    assert_eq!(
        db.telegram_link_status(&other).await.unwrap()["linked"],
        false
    );
}

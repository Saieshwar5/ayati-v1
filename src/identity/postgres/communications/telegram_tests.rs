//! Telegram end to end against a local fake Bot API: connecting a workspace,
//! creating agents' bots as managed bots, and sending replies through them.
use super::super::{tests::*, *};
use super::telegram_bots::NotConnected;
use super::telegram_fake::*;
use crate::identity::communications::{
    adapters::TransportAdapter,
    gateway,
    telegram::{poller, transport::TelegramTransport},
};
use std::sync::Arc;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn telegram_owner_connects_once_and_creates_each_agents_bot() {
    let db = fixture().await;
    let h = harness().await;
    let owner = user(&db, "tg-owner").await;
    let w = workspace(&db, &owner).await;
    let (_, nova) = super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    let (_, atlas) = super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    let admin = user(&db, "tg-admin").await;
    join(&db, &owner, &w, &admin, "Member").await;
    db.change(&owner, &w, &admin.id, Some("Admin"), None)
        .await
        .unwrap();
    let member = user(&db, "tg-member").await;
    join(&db, &owner, &w, &member, "Member").await;

    // Off by default, and nothing can be created before an admin connects Telegram.
    let off = db
        .telegram_status(&owner, &w, &nova, true, MANAGER)
        .await
        .unwrap();
    assert_eq!(
        (off["state"].as_str(), off["connected"].as_bool()),
        (Some("off"), Some(false))
    );
    assert!(
        db.telegram_activate(&owner, &w, &nova, MANAGER)
            .await
            .err()
            .unwrap()
            .is::<NotConnected>()
    );
    assert!(
        db.telegram_connect_code(&member, &w).await.is_err(),
        "only admins connect"
    );

    h.start(
        &db,
        &link(&db.telegram_connect_code(&owner, &w).await.unwrap()),
        OWNER_TG,
    )
    .await;
    assert!(h.fake.texts(MANAGER_ID, OWNER_TG)[0].contains("You now create the Telegram bots"));
    let ws = db.telegram_workspace(&owner, &w, true).await.unwrap();
    assert_eq!(
        (ws["connected"].as_bool(), ws["is_owner"].as_bool()),
        (Some(true), Some(true))
    );
    assert_eq!(
        db.telegram_workspace(&member, &w, true).await.unwrap()["can_manage"],
        false
    );

    // The owner switches it on: one tap in Telegram creates the bot.
    assert!(
        db.telegram_activate(&member, &w, &nova, MANAGER)
            .await
            .is_err(),
        "only the agent's owner or an admin"
    );
    let on = db
        .telegram_activate(&owner, &w, &nova, MANAGER)
        .await
        .unwrap();
    assert!(on.ask.is_none(), "the owner taps the link themselves");
    assert_eq!(on.view["state"], "requested");
    let create = on.view["create_link"].as_str().unwrap();
    assert!(
        create.starts_with(&format!("https://t.me/newbot/{MANAGER}/")),
        "{create}"
    );
    assert!(create.contains("?name="));
    let suggested = create
        .split('/')
        .nth(5)
        .unwrap()
        .split('?')
        .next()
        .unwrap()
        .to_owned();
    // Another admin switching it on asks the owner in Telegram instead.
    let asked = db
        .telegram_activate(&admin, &w, &atlas, MANAGER)
        .await
        .unwrap();
    let ask = asked.ask.unwrap();
    assert_eq!(ask.telegram_id, OWNER_TG);
    assert!(crate::identity::communications::telegram::settings::bot_username(&ask.username));
    assert!(
        asked.view["create_link"].is_null(),
        "only the owner gets the creation link"
    );

    // A bot someone else created is not attached to any agent.
    let stray = h.create(&db, STRANGER_TG, 5009, "stray_bot").await;
    assert_eq!(stray.created, 0);
    assert!(h.fake.texts(MANAGER_ID, STRANGER_TG)[0].contains("couldn't match"));

    let round = h.create(&db, OWNER_TG, 5001, &suggested).await;
    assert_eq!(round.created, 1);
    let active = db.telegram_status(&member, &w, &nova, true, MANAGER).await;
    assert!(
        active.is_err(),
        "people who may not talk to the agent don't see it"
    );
    db.chat_grant(&owner, &w, &member.id, &nova, true)
        .await
        .unwrap();
    let active = db
        .telegram_status(&member, &w, &nova, true, MANAGER)
        .await
        .unwrap();
    assert_eq!(
        (
            active["state"].as_str(),
            active["username"].as_str(),
            active["can_manage"].as_bool()
        ),
        (Some("active"), Some(suggested.as_str()), Some(false))
    );
    assert_eq!(active["link"], format!("https://t.me/{suggested}"));
    // The bot's profile is set from the agent, with the bot's own token.
    assert_eq!(h.fake.calls(5001, "setMyDescription").len(), 1);
    assert!(
        h.fake
            .texts(MANAGER_ID, OWNER_TG)
            .last()
            .unwrap()
            .contains(&suggested)
    );
    // The same bot never goes to a second agent.
    assert_eq!(h.create(&db, OWNER_TG, 5001, &suggested).await.created, 0);

    // Switching off and on brings the same bot back as a new generation.
    assert_eq!(
        db.telegram_deactivate(&owner, &w, &nova, MANAGER)
            .await
            .unwrap()["state"],
        "off"
    );
    let back = db
        .telegram_activate(&owner, &w, &nova, MANAGER)
        .await
        .unwrap();
    assert_eq!(
        (back.view["state"].as_str(), back.view["username"].as_str()),
        (Some("active"), Some(suggested.as_str()))
    );
    let revision: i64 = query(&db.db, "SELECT e.revision FROM relay_communication_endpoints e JOIN relay_telegram_bots b ON b.endpoint=e.id WHERE b.bot_id=5001", vec![])
        .await.unwrap().unwrap().try_get("", "revision").unwrap();
    assert_eq!(revision, 3);
    // A request switched off before the bot exists is dropped.
    db.telegram_deactivate(&admin, &w, &atlas, MANAGER)
        .await
        .unwrap();
    assert_eq!(
        db.telegram_status(&owner, &w, &atlas, true, MANAGER)
            .await
            .unwrap()["state"],
        "off"
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn telegram_replies_go_out_through_the_agents_own_bot() {
    let db = fixture().await;
    let h = harness().await;
    let (owner, w, _) = connected(&db, &h, "tg-send-owner", 7001).await;
    let (bot, owner_tg) = (7001, OWNER_TG + 7001);
    let adapters: Vec<Arc<dyn TransportAdapter>> =
        vec![Arc::new(TelegramTransport::with_file(h.file.clone()))];

    h.fake.message(
        bot,
        owner_tg,
        (owner_tg, "private"),
        Some("What is my leave balance?"),
    );
    poller::run_once(&db, &h.settings).await;
    let job = db.claim_job().await.unwrap().unwrap();
    db.chat_finish(&job, "You have **12 days** left <today>.", "complete")
        .await
        .unwrap();
    assert_eq!(gateway::run_once(&db, &adapters).await.sent, 1);
    let sent = h
        .fake
        .calls(bot, "sendMessage")
        .into_iter()
        .find(|v| v["parse_mode"] == "HTML")
        .unwrap();
    assert_eq!(
        (sent["chat_id"].as_i64(), sent["text"].as_str()),
        (
            Some(owner_tg),
            Some("You have <b>12 days</b> left &lt;today&gt;.")
        )
    );
    let state = |id: String| {
        let db = &db;
        let w = w.clone();
        async move {
            json_rows(&db.db, "SELECT to_jsonb(o) AS data FROM relay_communication_outbox o WHERE workspace=$1 AND id=$2", vec![w.into(), id.into()]).await.unwrap().remove(0)
        }
    };
    let out = state(job.reply.clone()).await;
    assert_eq!(
        (out["state"].as_str(), out["provider_id"].as_str()),
        (Some("accepted"), Some("77"))
    );

    // Refused formatting is sent once as plain text; a blocked bot is a failed send.
    *h.fake.refuse_html.lock().unwrap() = true;
    h.fake
        .message(bot, owner_tg, (owner_tg, "private"), Some("Again"));
    poller::run_once(&db, &h.settings).await;
    let job = db.claim_job().await.unwrap().unwrap();
    db.chat_finish(&job, "**Plain** now", "complete")
        .await
        .unwrap();
    gateway::run_once(&db, &adapters).await;
    assert_eq!(state(job.reply.clone()).await["state"], "accepted");
    assert!(
        h.fake
            .calls(bot, "sendMessage")
            .iter()
            .any(|v| v["text"] == "Plain now" && v.get("parse_mode").is_none())
    );
    *h.fake.blocked.lock().unwrap() = true;
    h.fake
        .message(bot, owner_tg, (owner_tg, "private"), Some("Once more"));
    poller::run_once(&db, &h.settings).await;
    let job = db.claim_job().await.unwrap().unwrap();
    db.chat_finish(&job, "Answer", "complete").await.unwrap();
    gateway::run_once(&db, &adapters).await;
    let out = state(job.reply.clone()).await;
    assert_eq!(out["state"], "failed");
    assert!(out["reason"].as_str().unwrap().contains("blocked"));

    // Admins can't bind Telegram senders by hand; people link their own accounts.
    let endpoint: String = query(
        &db.db,
        "SELECT endpoint FROM relay_telegram_bots WHERE bot_id=$1",
        vec![bot.into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "endpoint")
    .unwrap();
    let binding = crate::identity::communications::model::Binding {
        sender: "424242".into(),
        user: owner.id.clone(),
    };
    assert!(
        db.communication_binding(&owner, &w, &endpoint, &binding)
            .await
            .is_err()
    );
}

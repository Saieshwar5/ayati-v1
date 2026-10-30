//! Messages to agents get delivered/seen ticks like messages to people (C2).
use super::{tests::*, *};

async fn ticks(db: &PgStore, user: &User, w: &str, chat: &str, id: &str) -> (i64, i64) {
    let messages = db.chat_messages(user, w, chat).await.unwrap()["messages"].clone();
    let message = messages
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == id)
        .cloned()
        .unwrap();
    (
        message["delivered"].as_i64().unwrap(),
        message["seen"].as_i64().unwrap(),
    )
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_request_is_delivered_when_queued_and_seen_once_an_attempt_starts() {
    let db = fixture().await;
    let owner = user(&db, "agent-receipts").await;
    let w = workspace(&db, &owner).await;
    let (c, agent) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    let ask = message("Please check the stock", Some(&agent));
    db.chat_send(&owner, &w, &c, &ask)
        .await
        .unwrap()
        .expect("agent work queued");
    assert_eq!(
        ticks(&db, &owner, &w, &c, &ask.id).await,
        (1, 0),
        "queued work is delivered, not yet seen"
    );

    let tx = db.db.begin().await.unwrap();
    super::agent_receipts::seen(&tx, &w, &c, &ask.id, &agent)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let first = query(
        &db.db,
        "SELECT seen_at FROM relay_agent_receipts WHERE message=$1",
        vec![ask.id.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get::<i64>("", "seen_at")
    .unwrap();
    // A retried attempt reads it again; the first time stays and no row is added.
    let tx = db.db.begin().await.unwrap();
    super::agent_receipts::seen(&tx, &w, &c, &ask.id, &agent)
        .await
        .unwrap();
    super::agent_receipts::delivered(&tx, &w, &c, &ask.id, &agent)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let rows = query(
        &db.db,
        "SELECT count(*) AS n, min(seen_at) AS seen FROM relay_agent_receipts WHERE message=$1",
        vec![ask.id.clone().into()],
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(rows.try_get::<i64>("", "n").unwrap(), 1);
    assert_eq!(rows.try_get::<i64>("", "seen").unwrap(), first);
    assert_eq!(ticks(&db, &owner, &w, &c, &ask.id).await, (1, 1));

    let details = db.receipt_details(&owner, &w, &c, &ask.id).await.unwrap();
    let receipts = details["receipts"].as_array().unwrap();
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0]["agent"], true);
    assert!(receipts[0]["seen_at"].is_i64());
}

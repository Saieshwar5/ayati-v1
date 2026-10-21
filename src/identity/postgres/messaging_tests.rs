use super::tests::{chat, fixture, join, message, user, workspace};
use super::*;
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn messaging_replay_receipts_and_tenant_boundaries() {
    let db = fixture().await;
    let owner = user(&db, "sync-owner").await;
    let member = user(&db, "sync-member").await;
    let outsider = user(&db, "sync-outsider").await;
    let w = workspace(&db, &owner).await;
    let other = workspace(&db, &outsider).await;
    join(&db, &owner, &w, &member, "Member").await;
    let c = chat(&db, &owner, &w, vec![member.id.clone()], vec![]).await;
    let initial = db.sync_page(&member, &w, None).await.unwrap();
    let cursor = initial["cursor"].as_i64();
    let m = message("durable event", None);
    db.chat_send(&owner, &w, &c, &m).await.unwrap();
    db.chat_send(&owner, &w, &c, &m).await.unwrap();
    let delta = db.sync_page(&member, &w, cursor).await.unwrap();
    assert_eq!(delta["events"].as_array().unwrap().len(), 1);
    assert_eq!(delta["events"][0]["message"]["id"], m.id);
    assert!(db.sync_page(&outsider, &w, cursor).await.is_err());
    assert!(
        db.acknowledge(&outsider, &other, &c, std::slice::from_ref(&m.id), true)
            .await
            .is_err()
    );
    db.acknowledge(&member, &w, &c, std::slice::from_ref(&m.id), false)
        .await
        .unwrap();
    let received = db.accepted_message(&owner, &w, &c, &m.id).await.unwrap();
    assert_eq!(received["delivered"], 1);
    assert_eq!(received["seen"], 0);
    let receipt_cursor = db.sync_page(&member, &w, None).await.unwrap()["cursor"].as_i64();
    db.acknowledge(&member, &w, &c, std::slice::from_ref(&m.id), false)
        .await
        .unwrap();
    assert_eq!(
        db.sync_page(&member, &w, receipt_cursor).await.unwrap()["events"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    let mut reply = message("unopened thread", None);
    reply.thread = Some(m.id.clone());
    db.chat_send(&owner, &w, &c, &reply).await.unwrap();
    db.acknowledge(&member, &w, &c, std::slice::from_ref(&m.id), true)
        .await
        .unwrap();
    assert_eq!(
        db.accepted_message(&owner, &w, &c, &m.id).await.unwrap()["seen"],
        1
    );
    assert_eq!(
        db.accepted_message(&owner, &w, &c, &reply.id)
            .await
            .unwrap()["seen"],
        0
    );
    assert_eq!(
        db.unread(&member, &w).await.unwrap()["unread"][0]["count"],
        1
    );
    assert!(db.receipt_details(&member, &w, &c, &m.id).await.is_err());
    db.channel_members(&owner, &w, &c, std::slice::from_ref(&owner.id))
        .await
        .unwrap();
    let after_revoke = db.sync_page(&member, &w, cursor).await.unwrap();
    assert!(
        after_revoke["events"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["kind"] == "access")
    );
    assert!(
        db.acknowledge(&member, &w, &c, std::slice::from_ref(&m.id), true)
            .await
            .is_err()
    );
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn messaging_clock_does_not_publish_rolled_back_events() {
    let db = fixture().await;
    let owner = user(&db, "clock-owner").await;
    let w = workspace(&db, &owner).await;
    let c = chat(&db, &owner, &w, vec![], vec![]).await;
    let initial = db.sync_page(&owner, &w, None).await.unwrap()["cursor"].as_i64();
    let tx = db.workspace_tx(&w).await.unwrap();
    execute(&tx,"INSERT INTO relay_chat_messages(id,workspace,chat,author,kind,content,state,at) VALUES($1,$2,$3,$4,'user','rollback','sent',0)",vec![uuid::Uuid::new_v4().to_string().into(),w.clone().into(),c.clone().into(),owner.id.clone().into()]).await.unwrap();
    tx.rollback().await.unwrap();
    assert_eq!(
        db.sync_page(&owner, &w, initial).await.unwrap()["events"],
        serde_json::json!([])
    );
    let m = message("after rollback", None);
    db.chat_send(&owner, &w, &c, &m).await.unwrap();
    assert_eq!(
        db.sync_page(&owner, &w, initial).await.unwrap()["events"][0]["message"]["id"],
        m.id
    );
}

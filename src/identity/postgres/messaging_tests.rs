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

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn inline_mentions_and_timeline_agent_replies_keep_access_and_context_boundaries() {
    let db = fixture().await;
    let owner = user(&db, "mention-owner").await;
    let member = user(&db, "mention-member").await;
    let outsider = user(&db, "mention-outsider").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &member, "Member").await;
    join(&db, &owner, &w, &outsider, "Member").await;
    let agent = uuid::Uuid::new_v4().to_string();
    db.save_shared_agent(&owner, &w, &agent, "Helper", "Testing", "scripted")
        .await
        .unwrap();
    let c = chat(
        &db,
        &owner,
        &w,
        vec![member.id.clone()],
        vec![agent.clone()],
    )
    .await;
    db.chat_send(&owner, &w, &c, &message("UNRELATED-CONTEXT", None))
        .await
        .unwrap();
    let request = message(
        &format!(
            "[@Helper](mention:agent:{agent}) Ask [@Member](mention:user:{})",
            member.id
        ),
        Some(&agent),
    );
    let job = db
        .chat_send(&owner, &w, &c, &request)
        .await
        .unwrap()
        .unwrap();
    assert!(!job.prompt.contains("UNRELATED-CONTEXT"));
    let history = db.chat_messages(&member, &w, &c).await.unwrap();
    let reply = history["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == job.reply)
        .unwrap();
    assert!(reply["thread"].is_null());
    assert_eq!(reply["name"], "Helper");
    let replay = db.sync_page(&member, &w, Some(0)).await.unwrap();
    assert_eq!(
        replay["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|event| event["message"]["id"] == job.reply)
            .unwrap()["message"]["name"],
        "Helper"
    );
    assert!(
        db.chat_send(&owner, &w, &c, &request)
            .await
            .unwrap()
            .is_none()
    );
    let invalid = message(&format!("[@Outsider](mention:user:{})", outsider.id), None);
    assert!(db.chat_send(&owner, &w, &c, &invalid).await.is_err());
    // Channel members may ask the channel's agents without a grant (D231).
    assert!(
        db.chat_send(
            &member,
            &w,
            &c,
            &message(&format!("[@Helper](mention:agent:{agent})"), Some(&agent))
        )
        .await
        .is_ok()
    );
    let mut threaded = message("Explicit thread", Some(&agent));
    threaded.thread = Some(request.id.clone());
    let job = db
        .chat_send(&owner, &w, &c, &threaded)
        .await
        .unwrap()
        .unwrap();
    let history = db.chat_messages(&owner, &w, &c).await.unwrap();
    assert_eq!(
        history["messages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["id"] == job.reply)
            .unwrap()["thread"],
        request.id
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn the_listener_wakes_every_stream_when_it_connects_and_relays_each_change() {
    let db = fixture().await;
    let owner = user(&db, "listen-owner").await;
    let w = workspace(&db, &owner).await;
    let mut wake = db.changes.subscribe();
    let listening = tokio::spawn({
        let db = db.clone();
        async move { db.listen_changes().await }
    });
    let first = tokio::time::timeout(std::time::Duration::from_secs(10), wake.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first, "*", "connecting wakes every stream");
    // A committed message announces its workspace; the stream needs nothing else.
    let c = chat(&db, &owner, &w, vec![], vec![]).await;
    db.chat_send(&owner, &w, &c, &message("hello", None))
        .await
        .unwrap();
    let relayed = loop {
        let got = tokio::time::timeout(std::time::Duration::from_secs(10), wake.recv())
            .await
            .unwrap()
            .unwrap();
        if got == w {
            break got;
        }
    };
    assert_eq!(relayed, w);
    listening.abort();
}

/// Live streams re-check only every 30 seconds, so every database function
/// that writes a change record must also wake the listener's channel.
/// (`agent_pause.rs` writes one from Rust and notifies alongside it.)
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn every_change_record_trigger_wakes_live_streams() {
    let db = fixture().await;
    let functions = rows(
        &db.db,
        "SELECT p.proname, p.prosrc ~ 'pg_notify\\(\\s*''relay_message_changes''' AS wakes
           FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace
          WHERE n.nspname=current_schema() AND p.prosrc ~* 'insert\\s+into\\s+relay_sync_events'",
        vec![],
    )
    .await
    .unwrap();
    assert!(functions.len() >= 5, "the change record triggers are found");
    let silent: Vec<String> = functions
        .iter()
        .filter(|row| !row.try_get::<bool>("", "wakes").unwrap())
        .map(|row| row.try_get("", "proname").unwrap())
        .collect();
    assert!(
        silent.is_empty(),
        "these write change records without waking streams: {silent:?}"
    );
}

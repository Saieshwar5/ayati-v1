//! Schema 115: a thread's root reports its replies from the server, each new
//! or deleted reply refreshes the root on open screens, and a reply may also
//! be sent to the conversation.
use super::tests::*;
use super::*;
use crate::identity::chat::model::NewMessage;

fn reply(root: &str, text: &str, broadcast: bool) -> NewMessage {
    NewMessage {
        thread: Some(root.into()),
        broadcast,
        ..message(text, None)
    }
}
async fn root_of(db: &PgStore, user: &User, w: &str, chat: &str, id: &str) -> Value {
    db.chat_page(user, w, chat, None).await.unwrap()["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == id)
        .cloned()
        .unwrap()
}
async fn cursor(db: &PgStore, w: &str) -> i64 {
    query(
        &db.db,
        "SELECT cursor FROM relay_sync_clocks WHERE workspace=$1",
        vec![w.into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "cursor")
    .unwrap()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn thread_roots_count_their_replies_and_refresh_when_one_arrives_or_goes() {
    let db = fixture().await;
    let owner = user(&db, "thread-owner").await;
    let w = workspace(&db, &owner).await;
    let member = user(&db, "thread-member").await;
    join(&db, &owner, &w, &member, "Member").await;
    let c = chat(&db, &owner, &w, vec![member.id.clone()], vec![]).await;
    let root = message("Which truck goes to Pune?", None);
    db.chat_send(&owner, &w, &c, &root).await.unwrap();
    let first = root_of(&db, &member, &w, &c, &root.id).await;
    assert_eq!(
        (
            first["thread_replies"].as_i64(),
            first["thread_last"].is_null()
        ),
        (Some(0), true)
    );

    // A reply records a change for its root, so open screens refresh the count.
    let before = cursor(&db, &w).await;
    let one = reply(&root.id, "The 4 PM one", false);
    db.chat_send(&member, &w, &c, &one).await.unwrap();
    let page = db.sync_page(&owner, &w, Some(before)).await.unwrap();
    let changed: Vec<&Value> = page["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["message"]["id"] == root.id.as_str())
        .collect();
    assert!(!changed.is_empty(), "the root is part of the live change");
    assert_eq!(changed[0]["message"]["thread_replies"], 1);

    let two = reply(&root.id, "Thanks", false);
    db.chat_send(&owner, &w, &c, &two).await.unwrap();
    let summary = root_of(&db, &member, &w, &c, &root.id).await;
    assert_eq!(summary["thread_replies"], 2);
    assert!(summary["thread_last"].as_i64().unwrap() > 0);

    // Deleting a reply takes it out of the count, and refreshes the root again.
    let before = cursor(&db, &w).await;
    db.delete_message(&owner, &w, &c, &two.id).await.unwrap();
    assert_eq!(
        root_of(&db, &member, &w, &c, &root.id).await["thread_replies"],
        1
    );
    let page = db.sync_page(&owner, &w, Some(before)).await.unwrap();
    assert!(
        page["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["message"]["id"] == root.id.as_str() && e["message"]["thread_replies"] == 1)
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_thread_reply_can_also_go_to_the_conversation() {
    let db = fixture().await;
    let owner = user(&db, "broadcast-owner").await;
    let w = workspace(&db, &owner).await;
    let c = chat(&db, &owner, &w, vec![], vec![]).await;
    let root = message("Dispatch plan for Friday", None);
    db.chat_send(&owner, &w, &c, &root).await.unwrap();

    // Only a thread reply can be sent to the conversation as well.
    let loose = NewMessage {
        broadcast: true,
        ..message("Not in a thread", None)
    };
    let refused = db.chat_send(&owner, &w, &c, &loose).await.unwrap_err();
    assert!(refused.to_string().contains("Only a thread reply"));

    let shared = reply(&root.id, "Truck leaves at 4 PM, everyone", true);
    db.chat_send(&owner, &w, &c, &shared).await.unwrap();
    let saved = root_of(&db, &owner, &w, &c, &shared.id).await;
    assert_eq!(
        (saved["broadcast"].as_bool(), saved["thread"].as_str()),
        (Some(true), Some(root.id.as_str()))
    );
    assert_eq!(
        root_of(&db, &owner, &w, &c, &root.id).await["thread_replies"],
        1,
        "it still counts as a reply"
    );

    // A retry of the same message must carry the same choice.
    assert!(db.chat_send(&owner, &w, &c, &shared).await.is_ok());
    let changed = NewMessage {
        id: shared.id.clone(),
        ..reply(&root.id, "Truck leaves at 4 PM, everyone", false)
    };
    let refused = db.chat_send(&owner, &w, &c, &changed).await.unwrap_err();
    assert!(refused.to_string().contains("Message ID already used"));
}

//! What phones are told, and when, against real PostgreSQL (mobile plan M4).
use super::super::{
    execute,
    tests::{chat, fixture, join, message, user, workspace},
    *,
};
use super::{Device, push::Due};
use crate::identity::chat::model::NewChat;
use crate::identity::{digest, random_token};

/// A signed-in phone for this person with notifications on. Returns its token digest.
async fn phone(db: &PgStore, person: &User, provider: &str, expo: &str) -> String {
    let code = random_token();
    let verifier = random_token();
    db.issue_mobile_code(
        provider,
        &person.email,
        &digest(&code),
        "session_phone",
        &digest(&verifier),
    )
    .await
    .unwrap();
    let token = digest(&random_token());
    let device = Device {
        name: "Test phone".into(),
        platform: "ios".into(),
    };
    db.redeem_mobile_code(&digest(&code), &verifier, &token, &device)
        .await
        .unwrap();
    db.register_push(&token, expo, None).await.unwrap();
    token
}

/// Move messages past the reading delay, and joins further back so the
/// messages still count as arriving after the person joined (unread.rs).
async fn age(db: &PgStore, w: &str) {
    for sql in [
        "UPDATE relay_members SET joined_ms=joined_ms-120000 WHERE workspace=$1",
        "UPDATE relay_chat_people SET joined_ms=joined_ms-120000 WHERE workspace=$1",
        "UPDATE relay_chat_messages SET at=at-30 WHERE workspace=$1",
    ] {
        execute(&db.db, sql, vec![w.into()]).await.unwrap();
    }
}

async fn due_for(db: &PgStore, person: &User) -> Vec<Due> {
    db.push_due()
        .await
        .unwrap()
        .into_iter()
        .filter(|d| d.user == person.id)
        .collect()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn phones_hear_about_mentions_and_direct_messages_once_and_only_while_unread() {
    let db = fixture().await;
    let owner = user(&db, "push-owner").await;
    let member = user(&db, "push-member").await;
    let quiet = user(&db, "push-no-phone").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &member, "Member").await;
    join(&db, &owner, &w, &quiet, "Member").await;
    phone(&db, &member, "push-member", "ExponentPushToken[member]").await;
    let channel = chat(
        &db,
        &owner,
        &w,
        vec![member.id.clone(), quiet.id.clone()],
        vec![],
    )
    .await;

    // A channel message that doesn't mention them stays quiet (default: mentions).
    db.chat_send(&owner, &w, &channel, &message("Morning all", None))
        .await
        .unwrap();
    let mention = format!("Can you check this, [@Member](mention:user:{})?", member.id);
    db.chat_send(&owner, &w, &channel, &message(&mention, None))
        .await
        .unwrap();
    // Within the reading delay nothing is due yet.
    assert!(due_for(&db, &member).await.is_empty());
    age(&db, &w).await;
    let due = due_for(&db, &member).await;
    assert_eq!(due.len(), 1, "{due:?}");
    assert_eq!(
        (due[0].kind.as_str(), due[0].reason.as_str()),
        ("message", "mentioned")
    );
    assert_eq!(due[0].chat, channel);
    assert_eq!(due[0].workspace_name, "Test business");
    // Nobody without a phone is ever due.
    assert!(due_for(&db, &quiet).await.is_empty());

    // Claimed once, by whichever controller is first.
    assert!(db.claim_push(&due[0]).await.unwrap());
    assert!(!db.claim_push(&due[0]).await.unwrap());
    assert!(due_for(&db, &member).await.is_empty());

    // A direct message is due; once read elsewhere it is not.
    let direct = db
        .chat_create(
            &owner,
            &w,
            &NewChat {
                kind: "direct".into(),
                description: String::new(),
                id: uuid::Uuid::new_v4().to_string(),
                name: "Member".into(),
                users: vec![member.id.clone()],
                agents: vec![],
            },
        )
        .await
        .unwrap();
    let read = message("Lunch?", None);
    db.chat_send(&owner, &w, &direct, &read).await.unwrap();
    db.chat_send(&owner, &w, &direct, &message("Or coffee?", None))
        .await
        .unwrap();
    execute(
        &db.db,
        "INSERT INTO relay_message_receipts(workspace,chat,message,user_id,delivered_at,seen_at) VALUES($1,$2,$3,$4,1,1)",
        vec![w.clone().into(), direct.clone().into(), read.id.clone().into(), member.id.clone().into()],
    )
    .await
    .unwrap();
    age(&db, &w).await;
    let due = due_for(&db, &member).await;
    assert_eq!(due.len(), 1, "only the unread one: {due:?}");
    assert_eq!(due[0].reason, "direct");

    // A conversation set to "none" never notifies.
    execute(
        &db.db,
        "INSERT INTO relay_chat_notify(workspace,user_id,chat,level) VALUES($1,$2,$3,'none')",
        vec![
            w.clone().into(),
            member.id.clone().into(),
            direct.clone().into(),
        ],
    )
    .await
    .unwrap();
    assert!(due_for(&db, &member).await.is_empty());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn signing_out_or_moving_the_token_removes_the_registration() {
    let db = fixture().await;
    let owner = user(&db, "push-move-owner").await;
    let member = user(&db, "push-move-member").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &member, "Member").await;
    let first = phone(
        &db,
        &member,
        "push-move-member",
        "ExponentPushToken[same-phone]",
    )
    .await;
    assert_eq!(
        db.push_tokens(&member.id).await.unwrap(),
        ["ExponentPushToken[same-phone]"]
    );
    let listed = db.devices(&member, Some(&first)).await.unwrap();
    assert_eq!(listed[0]["notifications"], true);
    // The same phone signs in again: the token moves to the new session.
    let second = phone(
        &db,
        &member,
        "push-move-member",
        "ExponentPushToken[same-phone]",
    )
    .await;
    assert_eq!(db.push_tokens(&member.id).await.unwrap().len(), 1);
    // Signing the old session out leaves the new registration.
    db.logout_device(&first).await.unwrap();
    assert_eq!(db.push_tokens(&member.id).await.unwrap().len(), 1);
    db.logout_device(&second).await.unwrap();
    assert!(db.push_tokens(&member.id).await.unwrap().is_empty());
    // Invalid tokens are refused; a phone can turn notifications off.
    let third = phone(&db, &member, "push-move-member", "ExponentPushToken[third]").await;
    assert!(db.register_push(&third, "not-a-token", None).await.is_err());
    db.remove_push(&third).await.unwrap();
    assert!(db.push_tokens(&member.id).await.unwrap().is_empty());
    let _ = w;
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn the_sender_pushes_who_and_what_kind_and_drops_tokens_expo_rejects() {
    use axum::{Json, Router, extract::State, routing::post};
    use serde_json::{Value, json};
    use std::sync::{Arc, Mutex};
    let received: Arc<Mutex<Vec<Value>>> = Default::default();
    let routes = Router::new()
        .route(
            "/push",
            post(|State(seen): State<Arc<Mutex<Vec<Value>>>>, Json(batch): Json<Vec<Value>>| async move {
                // The second phone no longer takes notifications.
                let tickets: Vec<Value> = batch.iter().map(|m| if m["to"] == "ExponentPushToken[gone]" {
                    json!({"status": "error", "details": {"error": "DeviceNotRegistered"}})
                } else {
                    json!({"status": "ok", "id": "ticket"})
                }).collect();
                seen.lock().unwrap().extend(batch);
                Json(json!({"data": tickets}))
            }),
        )
        .with_state(received.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, routes).await.unwrap() });

    let store = fixture().await;
    let owner = user(&store, "push-send-owner").await;
    let member = user(&store, "push-send-member").await;
    let w = workspace(&store, &owner).await;
    join(&store, &owner, &w, &member, "Member").await;
    phone(
        &store,
        &member,
        "push-send-member",
        "ExponentPushToken[kept]",
    )
    .await;
    phone(
        &store,
        &member,
        "push-send-member",
        "ExponentPushToken[gone]",
    )
    .await;
    let channel = chat(&store, &owner, &w, vec![member.id.clone()], vec![]).await;
    let secret = format!(
        "The safe code is 4471, [@Member](mention:user:{})",
        member.id
    );
    store
        .chat_send(&owner, &w, &channel, &message(&secret, None))
        .await
        .unwrap();
    age(&store, &w).await;

    let settings = crate::config::Push {
        url: format!("http://{address}/push"),
        token_file: None,
    };
    let app = crate::identity::App {
        library: Default::default(),
        provider: crate::identity::Provider::test_provider(),
        store,
        config: std::sync::Arc::new(crate::config::Controller::for_tests(
            "http://127.0.0.1:8765",
        )),
        runtimes: crate::identity::workspaces::Runtimes::new(
            std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()),
        ),
        simulation: None,
    };
    let sender = crate::identity::push::Sender::new(&settings).unwrap();
    assert_eq!(
        crate::identity::push::run_once(&app, &sender)
            .await
            .unwrap(),
        1
    );
    let batch = received.lock().unwrap().clone();
    assert_eq!(batch.len(), 2, "one per phone");
    assert_eq!(
        batch[0]["title"],
        "push-send-owner@example.test mentioned you"
    );
    assert_eq!(batch[0]["body"], "Test business");
    assert_eq!(batch[0]["data"]["chat"], channel.as_str());
    assert!(
        !serde_json::to_string(&batch).unwrap().contains("4471"),
        "message text never leaves"
    );
    assert_eq!(
        app.store.push_tokens(&member.id).await.unwrap(),
        ["ExponentPushToken[kept]"]
    );
    // Nothing is sent twice.
    assert_eq!(
        crate::identity::push::run_once(&app, &sender)
            .await
            .unwrap(),
        0
    );
    assert_eq!(received.lock().unwrap().len(), 2);
    server.abort();
}

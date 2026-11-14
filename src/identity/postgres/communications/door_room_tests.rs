//! E3: doors with proven identity share the person's direct room with the
//! agent. The agent remembers a person across Chat and WhatsApp, two people
//! never share a room, a door never answers a question, and unlinking stops
//! the door while the room stays.
use super::super::chat::updates::Update;
use super::super::{tests::*, *};
use super::whatsapp_people::Outcome;
use super::whatsapp_tests::{FIRST, MEMBER_PHONE, OWNER_PHONE, link, message, offer};
use crate::identity::chat::model::NewMessage;

fn chat(agent: &str, text: &str) -> NewMessage {
    NewMessage {
        attachments: vec![],
        id: uuid::Uuid::new_v4().to_string(),
        content: text.into(),
        agent: Some(agent.into()),
        agents: vec![],
        thread: None,
        reply_to: None,
    }
}
async fn outbox_state(db: &PgStore, w: &str, reply: &str) -> Option<(String, String)> {
    query(
        &db.db,
        "SELECT state,transport FROM relay_communication_outbox WHERE workspace=$1 AND id=$2",
        vec![w.into(), reply.into()],
    )
    .await
    .unwrap()
    .map(|r| {
        (
            r.try_get("", "state").unwrap(),
            r.try_get("", "transport").unwrap(),
        )
    })
}
async fn send_update_state(db: &PgStore, job: &crate::identity::chat::model::Job) -> String {
    db.send_update(
        job,
        &Update {
            text: "Working on it".into(),
        },
    )
    .await
    .unwrap()["state"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn proven_doors_share_the_persons_direct_room_and_memory() {
    let db = fixture().await;
    let owner = user(&db, "door-owner").await;
    let w = workspace(&db, &owner).await;
    let (_, nova) = super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    db.whatsapp_activate(&owner, &w, &nova, &offer(&[FIRST]))
        .await
        .unwrap();
    link(&db, &owner, OWNER_PHONE).await;

    // A WhatsApp question lands in the person's direct room with the agent.
    let (outcome, _) = db
        .whatsapp_incoming(&message("D1", OWNER_PHONE, FIRST, "WA_FIRST_QUESTION"))
        .await
        .unwrap();
    assert_eq!(outcome, Outcome::Admitted);
    let first = db.claim_job().await.unwrap().unwrap();
    let room = first.chat.clone();
    let kind: String = query(
        &db.db,
        "SELECT kind FROM relay_chats WHERE workspace=$1 AND id=$2",
        vec![w.clone().into(), room.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "kind")
    .unwrap();
    assert_eq!(kind, "direct");
    assert_eq!(
        send_update_state(&db, &first).await,
        "not_here",
        "a WhatsApp request gets one complete reply"
    );
    db.chat_finish(&first, "WA_FIRST_ANSWER", "complete")
        .await
        .unwrap();
    assert_eq!(
        outbox_state(&db, &w, &first.reply).await,
        Some(("queued".into(), "whatsapp".into())),
        "the reply leaves through the door it came in by"
    );

    // The same person continues in Chat: the agent remembers the WhatsApp
    // question, keeps Chat's rhythm and answers only in Chat.
    let second = db
        .chat_send(&owner, &w, &room, &chat(&nova, "CHAT_FOLLOWUP"))
        .await
        .unwrap()
        .unwrap();
    let claimed = db.claim_job().await.unwrap().unwrap();
    assert_eq!(claimed.reply, second.reply);
    let second = claimed;
    assert!(
        second.prompt.contains("WA_FIRST_QUESTION") && second.prompt.contains("WA_FIRST_ANSWER")
    );
    assert_ne!(send_update_state(&db, &second).await, "not_here");
    db.chat_finish(&second, "CHAT_ANSWER", "complete")
        .await
        .unwrap();
    assert_eq!(
        outbox_state(&db, &w, &second.reply)
            .await
            .map(|(_, t)| t)
            .as_deref(),
        Some("chat")
    );

    // And the reverse: WhatsApp remembers Chat.
    db.whatsapp_incoming(&message("D2", OWNER_PHONE, FIRST, "WA_SECOND"))
        .await
        .unwrap();
    let third = db.claim_job().await.unwrap().unwrap();
    assert_eq!(third.chat, room);
    assert!(third.prompt.contains("CHAT_FOLLOWUP") && third.prompt.contains("CHAT_ANSWER"));

    // Chat names the door on each message.
    let page = db.chat_page(&owner, &w, &room, None).await.unwrap();
    let doors: Vec<(String, Option<String>)> = page["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["update_of"].is_null())
        .map(|m| {
            (
                m["content"].as_str().unwrap_or_default().to_owned(),
                m["door"].as_str().map(str::to_owned),
            )
        })
        .collect();
    for (text, door) in &doors {
        let expected = text.starts_with("WA_").then(|| "whatsapp".to_owned());
        if !text.is_empty() {
            assert_eq!(door, &expected, "{text}");
        }
    }

    // A question waits for an answer inside Cuckoo. A WhatsApp message saying
    // the answer is a new request, never the answer.
    let question = db
        .ask_question(&third, &super::super::attention_tests::ask())
        .await
        .unwrap();
    db.chat_finish(&third, "Waiting for your choice in Cuckoo", "complete")
        .await
        .unwrap();
    db.whatsapp_incoming(&message("D3", OWNER_PHONE, FIRST, "road"))
        .await
        .unwrap();
    assert_eq!(
        db.question(&owner, &w, question["id"].as_str().unwrap())
            .await
            .unwrap()["state"],
        "pending"
    );
    let fourth = db.claim_job().await.unwrap().unwrap();
    assert_eq!(fourth.chat, room);
    db.chat_finish(&fourth, "WA_FOURTH_ANSWER", "complete")
        .await
        .unwrap();

    // Unlinking stops the door: the unsent reply is cancelled and new
    // messages are held. The room and Chat keep working.
    db.mobile_unlink(&owner).await.unwrap();
    assert_eq!(
        outbox_state(&db, &w, &fourth.reply)
            .await
            .map(|(s, _)| s)
            .as_deref(),
        Some("cancelled")
    );
    let (outcome, _) = db
        .whatsapp_incoming(&message("D4", OWNER_PHONE, FIRST, "Still me"))
        .await
        .unwrap();
    assert_eq!(outcome, Outcome::Held);
    assert!(
        db.chat_send(&owner, &w, &room, &chat(&nova, "Chat still works"))
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn two_people_on_one_agents_door_keep_separate_rooms_and_memory() {
    let db = fixture().await;
    let owner = user(&db, "door-two-owner").await;
    let w = workspace(&db, &owner).await;
    let (_, nova) = super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    let member = user(&db, "door-two-member").await;
    join(&db, &owner, &w, &member, "Member").await;
    db.chat_grant(&owner, &w, &member.id, &nova, true)
        .await
        .unwrap();
    db.whatsapp_activate(&owner, &w, &nova, &offer(&[FIRST]))
        .await
        .unwrap();
    link(&db, &owner, OWNER_PHONE).await;
    link(&db, &member, MEMBER_PHONE).await;

    db.whatsapp_incoming(&message("T1", OWNER_PHONE, FIRST, "OWNER_SECRET"))
        .await
        .unwrap();
    let mine = db.claim_job().await.unwrap().unwrap();
    db.chat_finish(&mine, "OWNER_SECRET_ANSWER", "complete")
        .await
        .unwrap();
    db.whatsapp_incoming(&message("T2", MEMBER_PHONE, FIRST, "Member question"))
        .await
        .unwrap();
    let theirs = db.claim_job().await.unwrap().unwrap();
    assert_eq!(theirs.requester, member.id);
    assert_ne!(theirs.chat, mine.chat);
    assert!(!theirs.prompt.contains("OWNER_SECRET"));
    assert!(db.chat_page(&member, &w, &mine.chat, None).await.is_err());
}

//! E3: doors with proven identity share the person's direct room with the
//! agent. The agent remembers a person across Chat and email, two people
//! never share a room, a door never answers a question, and switching the
//! door off stops it while the room stays.
use super::super::chat::updates::Update;
use super::super::{tests::*, *};
use super::email_people::{Incoming, Outcome};
use crate::identity::chat::model::NewMessage;
use crate::identity::communications::email::inbound::Mail;

const DOMAIN: &str = "mail.door.example";

fn mail(from: &str, text: &str) -> Mail {
    Mail {
        message_id: Some(format!("{}@acme.example", uuid::Uuid::new_v4())),
        from: Some(from.into()),
        subject: "Question".into(),
        text: text.into(),
        parents: vec![],
        automated: false,
        attachments: 0,
    }
}
async fn arrive(db: &PgStore, event: &str, to: &str, from: &str, text: &str) -> Outcome {
    db.email_incoming(&Incoming {
        event: event.into(),
        to: to.into(),
        dmarc: true,
        mail: &mail(from, text),
    })
    .await
    .unwrap()
}
async fn address(db: &PgStore, owner: &User, w: &str, agent: &str) -> String {
    db.email_activate(owner, w, agent, DOMAIN).await.unwrap()["address"]
        .as_str()
        .unwrap()
        .to_owned()
}

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
    let to = address(&db, &owner, &w, &nova).await;
    let me = owner.email.clone();

    // An email question lands in the person's direct room with the agent.
    assert_eq!(
        arrive(&db, "D1", &to, &me, "MAIL_FIRST_QUESTION").await,
        Outcome::Admitted
    );
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
        "an email request gets one complete reply"
    );
    db.chat_finish(&first, "MAIL_FIRST_ANSWER", "complete")
        .await
        .unwrap();
    assert_eq!(
        outbox_state(&db, &w, &first.reply).await,
        Some(("queued".into(), "email".into())),
        "the reply leaves through the door it came in by"
    );

    // The same person continues in Chat: the agent remembers the email
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
        second.prompt.contains("MAIL_FIRST_QUESTION") && second.prompt.contains("MAIL_FIRST_ANSWER")
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

    // And the reverse: email remembers Chat.
    arrive(&db, "D2", &to, &me, "MAIL_SECOND").await;
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
        let expected = text.contains("MAIL_").then(|| "email".to_owned());
        if !text.is_empty() {
            assert_eq!(door, &expected, "{text}");
        }
    }

    // A question waits for an answer inside Cuckoo. An email saying
    // the answer is a new request, never the answer.
    let question = db
        .ask_question(&third, &super::super::attention_tests::ask())
        .await
        .unwrap();
    db.chat_finish(&third, "Waiting for your choice in Cuckoo", "complete")
        .await
        .unwrap();
    arrive(&db, "D3", &to, &me, "road").await;
    assert_eq!(
        db.question(&owner, &w, question["id"].as_str().unwrap())
            .await
            .unwrap()["state"],
        "pending"
    );
    let fourth = db.claim_job().await.unwrap().unwrap();
    assert_eq!(fourth.chat, room);
    db.chat_finish(&fourth, "MAIL_FOURTH_ANSWER", "complete")
        .await
        .unwrap();

    // Switching email off stops the door: the unsent reply is cancelled and
    // new mail gets no work. The room and Chat keep working.
    db.email_deactivate(&owner, &w, &nova).await.unwrap();
    assert_eq!(
        outbox_state(&db, &w, &fourth.reply)
            .await
            .map(|(s, _)| s)
            .as_deref(),
        Some("cancelled")
    );
    assert_ne!(
        arrive(&db, "D4", &to, &me, "Still me").await,
        Outcome::Admitted
    );
    assert!(db.claim_job().await.unwrap().is_none());
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
    let to = address(&db, &owner, &w, &nova).await;

    arrive(&db, "T1", &to, &owner.email, "OWNER_SECRET").await;
    let mine = db.claim_job().await.unwrap().unwrap();
    db.chat_finish(&mine, "OWNER_SECRET_ANSWER", "complete")
        .await
        .unwrap();
    arrive(&db, "T2", &to, &member.email, "Member question").await;
    let theirs = db.claim_job().await.unwrap().unwrap();
    assert_eq!(theirs.requester, member.id);
    assert_ne!(theirs.chat, mine.chat);
    assert!(!theirs.prompt.contains("OWNER_SECRET"));
    assert!(db.chat_page(&member, &w, &mine.chat, None).await.is_err());
}

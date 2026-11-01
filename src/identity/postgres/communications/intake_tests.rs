use super::super::{tests::*, *};
use crate::identity::chat::model::Job;
use crate::identity::communications::model::{
    Binding, Endpoint, Message, Outcome, Receipt, SetState, Transport,
};
use serde_json::json;

async fn setup() -> (PgStore, User, String, String, Endpoint) {
    let db = fixture().await;
    let owner = user(&db, "communications-owner").await;
    let w = workspace(&db, &owner).await;
    let (chat, agent) = super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    let endpoint = Endpoint {
        id: uuid::Uuid::new_v4().to_string(),
        agent,
        transport: Transport::Email,
        address: "assistant@simulation.invalid".into(),
    };
    db.communication_endpoint(&owner, &w, &endpoint)
        .await
        .unwrap();
    db.communication_binding(
        &owner,
        &w,
        &endpoint.id,
        &Binding {
            sender: "owner@simulation.invalid".into(),
            user: owner.id.clone(),
        },
    )
    .await
    .unwrap();
    (db, owner, w, chat, endpoint)
}
fn incoming(event: &str, text: &str) -> Message {
    Message {
        event: event.into(),
        sender: "owner@simulation.invalid".into(),
        recipient: "assistant@simulation.invalid".into(),
        conversation: "email-thread-1".into(),
        text: text.into(),
        attachments: vec![],
        automated: false,
        thread: None,
    }
}
async fn finish(db: &PgStore) -> Job {
    let job = db.claim_job().await.unwrap().unwrap();
    db.chat_finish(&job, "Private reply", "complete")
        .await
        .unwrap();
    job
}
async fn row(db: &PgStore, table: &str, w: &str) -> Value {
    json_rows(&db.db,&format!("SELECT to_jsonb(r) AS data FROM {table} r WHERE workspace=$1 ORDER BY sequence DESC LIMIT 1"),vec![w.into()]).await.unwrap().remove(0)
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn communications_chat_records_atomic_intake_and_publication_without_changing_retries() {
    let (db, owner, w, c, e) = setup().await;
    let request = message("Normal chat", Some(&e.agent));
    db.chat_send(&owner, &w, &c, &request).await.unwrap();
    assert!(
        db.chat_send(&owner, &w, &c, &request)
            .await
            .unwrap()
            .is_none()
    );
    let job = finish(&db).await;
    let input = row(&db, "relay_communication_inbox", &w).await;
    assert_eq!(input["transport"], "chat");
    assert_eq!(input["reply"], job.reply);
    let output = row(&db, "relay_communication_outbox", &w).await;
    assert_eq!(output["state"], "published");
    assert_eq!(output["envelope"]["recipient"], c);
    assert!(
        db.simulate_dispatch(&owner, &w, &job.reply, Outcome::Delivered)
            .await
            .is_err()
    );
    assert!(
        !db.finish_claimed_job(&job, "changed", "complete")
            .await
            .unwrap()
    );
    assert_eq!(row(&db, "relay_communication_outbox", &w).await, output);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn communications_deduplicate_concurrently_and_hold_unknown_senders_without_work() {
    let (db, owner, w, _, e) = setup().await;
    let input = incoming("event-1", "Known sender request");
    let (a, b) = tokio::join!(
        db.simulate_incoming(&owner, &w, &e.id, &input),
        db.simulate_incoming(&owner, &w, &e.id, &input)
    );
    assert_ne!(a.unwrap()["duplicate"], b.unwrap()["duplicate"]);
    let job = finish(&db).await;
    assert!(db.claim_job().await.unwrap().is_none());
    let mut changed = input.clone();
    changed.text = "Changed body".into();
    assert!(
        db.simulate_incoming(&owner, &w, &e.id, &changed)
            .await
            .is_err()
    );
    let mut unknown = incoming("event-unknown", "DO_NOT_RETAIN_UNKNOWN_CONTENT");
    unknown.sender = "unknown@simulation.invalid".into();
    assert_eq!(
        db.simulate_incoming(&owner, &w, &e.id, &unknown)
            .await
            .unwrap()["status"],
        "held"
    );
    assert!(
        !row(&db, "relay_communication_inbox", &w)
            .await
            .to_string()
            .contains("DO_NOT_RETAIN")
    );
    db.communication_binding(
        &owner,
        &w,
        &e.id,
        &Binding {
            sender: unknown.sender.clone(),
            user: owner.id.clone(),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        db.simulate_incoming(&owner, &w, &e.id, &unknown)
            .await
            .unwrap()["status"],
        "held"
    );
    assert!(db.claim_job().await.unwrap().is_none());
    let mut echo = incoming("echo", "Echo");
    echo.sender = e.address.clone();
    assert_eq!(
        db.simulate_incoming(&owner, &w, &e.id, &echo)
            .await
            .unwrap()["status"],
        "ignored"
    );
    let mut bot = incoming("auto", "Autoreply");
    bot.automated = true;
    assert_eq!(
        db.simulate_incoming(&owner, &w, &e.id, &bot).await.unwrap()["status"],
        "ignored"
    );
    assert!(db.claim_job().await.unwrap().is_none());
    assert_eq!(
        row(&db, "relay_communication_outbox", &w).await["id"],
        job.reply
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn communications_separate_people_threads_transports_and_deny_chat_audience_changes() {
    let (db, owner, w, regular, e) = setup().await;
    let other = user(&db, "same-display-name").await;
    join(&db, &owner, &w, &other, "Member").await;
    db.chat_grant(&owner, &w, &other.id, &e.agent, true)
        .await
        .unwrap();
    db.communication_binding(
        &owner,
        &w,
        &e.id,
        &Binding {
            sender: "other@simulation.invalid".into(),
            user: other.id.clone(),
        },
    )
    .await
    .unwrap();
    db.chat_send(
        &owner,
        &w,
        &regular,
        &message("PRIVATE_CHAT_SECRET", Some(&e.agent)),
    )
    .await
    .unwrap();
    finish(&db).await;
    db.simulate_incoming(
        &owner,
        &w,
        &e.id,
        &incoming("first", "PRIVATE_EMAIL_SECRET"),
    )
    .await
    .unwrap();
    let first = finish(&db).await;
    let mut input = incoming("second", "Tell me my context");
    input.sender = "other@simulation.invalid".into();
    db.simulate_incoming(&owner, &w, &e.id, &input)
        .await
        .unwrap();
    let second = db.claim_job().await.unwrap().unwrap();
    assert_ne!(first.chat, second.chat);
    assert!(!second.prompt.contains("PRIVATE_EMAIL_SECRET"));
    assert!(!second.prompt.contains("PRIVATE_CHAT_SECRET"));
    let (identity, _) = db.request_context(&second).await.unwrap();
    assert_eq!(identity["requesting_principal"]["id"], other.id);
    assert_eq!(identity["communication"]["transport"], "email");
    assert_eq!(identity["communication"]["simulated"], true);
    db.chat_finish(&second, "Done", "complete").await.unwrap();
    assert!(db.chat_messages(&other, &w, &first.chat).await.is_err());
    assert!(
        db.channel_members(
            &owner,
            &w,
            &first.chat,
            &[owner.id.clone(), other.id.clone()]
        )
        .await
        .is_err()
    );
    assert!(
        db.channel_settings(&owner, &w, &first.chat, "public", false, None)
            .await
            .is_err()
    );
    assert!(
        db.chat_send(
            &owner,
            &w,
            &first.chat,
            &message("Bypass transport", Some(&e.agent))
        )
        .await
        .is_err()
    );
    let mut next = incoming("new-thread", "Different thread");
    next.conversation = "email-thread-2".into();
    db.simulate_incoming(&owner, &w, &e.id, &next)
        .await
        .unwrap();
    let third = finish(&db).await;
    assert_ne!(third.chat, first.chat);
    let whatsapp = Endpoint {
        id: uuid::Uuid::new_v4().to_string(),
        agent: e.agent.clone(),
        transport: Transport::Whatsapp,
        address: "sim-agent-number".into(),
    };
    db.communication_endpoint(&owner, &w, &whatsapp)
        .await
        .unwrap();
    db.communication_binding(
        &owner,
        &w,
        &whatsapp.id,
        &Binding {
            sender: "owner@simulation.invalid".into(),
            user: owner.id.clone(),
        },
    )
    .await
    .unwrap();
    let mut wa = incoming("first", "WhatsApp has its own context");
    wa.recipient = whatsapp.address;
    db.simulate_incoming(&owner, &w, &whatsapp.id, &wa)
        .await
        .unwrap();
    let fourth = finish(&db).await;
    assert_ne!(fourth.chat, first.chat);
    assert!(!fourth.prompt.contains("PRIVATE_EMAIL_SECRET"));
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn communications_uncertain_send_never_retries_and_receipts_are_monotonic() {
    let (db, owner, w, _, e) = setup().await;
    db.simulate_incoming(&owner, &w, &e.id, &incoming("event", "Message"))
        .await
        .unwrap();
    let job = finish(&db).await;
    let sent = db
        .simulate_dispatch(&owner, &w, &job.reply, Outcome::Timeout)
        .await
        .unwrap();
    assert_eq!(sent["state"], "uncertain");
    assert_eq!(
        db.simulate_dispatch(&owner, &w, &job.reply, Outcome::Delivered)
            .await
            .unwrap(),
        sent
    );
    let attempt = sent["attempt"].as_str().unwrap();
    let mut receipt = Receipt {
        attempt: attempt.into(),
        provider_id: format!("sim:{}:{attempt}", job.reply),
        delivered: true,
    };
    assert_eq!(
        db.simulate_receipt(&owner, &w, &job.reply, &receipt)
            .await
            .unwrap()["state"],
        "delivered"
    );
    receipt.delivered = false;
    assert_eq!(
        db.simulate_receipt(&owner, &w, &job.reply, &receipt)
            .await
            .unwrap()["state"],
        "delivered"
    );
    receipt.attempt = "foreign-attempt".into();
    assert!(
        db.simulate_receipt(&owner, &w, &job.reply, &receipt)
            .await
            .is_err()
    );
    assert_eq!(
        row(&db, "relay_communication_outbox", &w).await["attempts"],
        1
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn communications_crashed_dispatch_expires_uncertain_but_definite_rejections_can_retry() {
    let (db, owner, w, _, e) = setup().await;
    db.simulate_incoming(&owner, &w, &e.id, &incoming("event", "Message"))
        .await
        .unwrap();
    let job = finish(&db).await;
    let failed = db
        .simulate_dispatch(&owner, &w, &job.reply, Outcome::Rejected)
        .await
        .unwrap();
    assert_eq!(failed["state"], "failed");
    let crashed = db
        .simulate_dispatch(&owner, &w, &job.reply, Outcome::Crash)
        .await
        .unwrap();
    assert_eq!(crashed["state"], "sending");
    assert_ne!(failed["attempt"], crashed["attempt"]);
    execute(
        &db.db,
        "UPDATE relay_communication_outbox SET lease_until=0 WHERE workspace=$1",
        vec![w.clone().into()],
    )
    .await
    .unwrap();
    assert_eq!(
        db.simulate_dispatch(&owner, &w, &job.reply, Outcome::Delivered)
            .await
            .unwrap()["state"],
        "uncertain"
    );
    assert_eq!(
        row(&db, "relay_communication_outbox", &w).await["attempts"],
        2
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn communications_revoke_generation_before_dispatch_and_do_not_revive_old_context() {
    let (db, owner, w, _, e) = setup().await;
    db.simulate_incoming(
        &owner,
        &w,
        &e.id,
        &incoming("event", "OLD_GENERATION_SECRET"),
    )
    .await
    .unwrap();
    let job = finish(&db).await;
    let off = SetState {
        revision: 1,
        enabled: false,
    };
    db.communication_state(&owner, &w, &e.id, None, &off)
        .await
        .unwrap();
    assert!(
        db.communication_state(&owner, &w, &e.id, None, &off)
            .await
            .is_err()
    );
    assert_eq!(
        db.simulate_dispatch(&owner, &w, &job.reply, Outcome::Delivered)
            .await
            .unwrap()["state"],
        "cancelled"
    );
    db.communication_state(
        &owner,
        &w,
        &e.id,
        None,
        &SetState {
            revision: 2,
            enabled: true,
        },
    )
    .await
    .unwrap();
    assert!(!super::source_access::allowed(&db.db, &job).await.unwrap());
    db.simulate_incoming(&owner, &w, &e.id, &incoming("event-new", "New request"))
        .await
        .unwrap();
    let next = finish(&db).await;
    assert_ne!(next.chat, job.chat);
    assert!(!next.prompt.contains("OLD_GENERATION_SECRET"));
    db.communication_state(&owner, &w, &e.id, Some("owner@simulation.invalid"), &off)
        .await
        .unwrap();
    assert_eq!(
        db.simulate_dispatch(&owner, &w, &next.reply, Outcome::Delivered)
            .await
            .unwrap()["state"],
        "cancelled"
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn communications_operator_metadata_and_config_deny_foreign_owners_and_members() {
    let (db, owner, w, _, e) = setup().await;
    let member = user(&db, "communications-member").await;
    join(&db, &owner, &w, &member, "Member").await;
    assert!(db.communication_status(&member, &w).await.is_err());
    assert!(db.communication_endpoint(&member, &w, &e).await.is_err());
    assert!(
        db.simulate_incoming(&member, &w, &e.id, &incoming("event", "SECRET"))
            .await
            .is_err()
    );
    let foreign = workspace(&db, &member).await;
    assert!(
        db.simulate_incoming(&member, &foreign, &e.id, &incoming("event", "SECRET"))
            .await
            .is_err()
    );
    let mut bad = incoming("event", "SECRET");
    bad.recipient = "another@simulation.invalid".into();
    assert!(db.simulate_incoming(&owner, &w, &e.id, &bad).await.is_err());
    db.simulate_incoming(&owner, &w, &e.id, &incoming("event", "SECRET"))
        .await
        .unwrap();
    finish(&db).await;
    let status = db
        .communication_status(&owner, &w)
        .await
        .unwrap()
        .to_string();
    assert!(!status.contains("SECRET"));
    assert!(!status.contains("Private reply"));
    assert!(serde_json::from_value::<Message>(json!({"event":"x","sender":"s","recipient":"r","conversation":"c","text":"x","user":owner.id})).is_err());
}

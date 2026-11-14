//! WhatsApp on Cuckoo's numbers end to end against a local fake Twilio:
//! switching an agent's number on and off, people linking their numbers, who
//! may write, separate rooms, sending the reply and delivery updates.
use super::super::{tests::*, *};
use super::whatsapp_numbers::{NoFreeNumber, Offer};
use super::whatsapp_people::{Incoming, Outcome};
use crate::identity::communications::{
    adapters::TransportAdapter, gateway, whatsapp::twilio::WhatsappTransport,
};
use axum::{Form, Router, extract::State, http::HeaderMap, routing::post};
use std::{
    os::unix::fs::PermissionsExt,
    sync::{Arc, Mutex},
};

pub(super) const FIRST: &str = "+14155550101";
const SECOND: &str = "+14155550102";
pub(super) const OWNER_PHONE: &str = "+919800000001";
pub(super) const MEMBER_PHONE: &str = "+919800000002";
/// A dummy account SID, built at run time so the source holds no SID-shaped text.
fn account() -> String {
    format!("AC{}", "0".repeat(32))
}

pub(super) fn offer(numbers: &[&str]) -> Offer {
    Offer {
        numbers: numbers.iter().map(|n| n.to_string()).collect(),
        cooling_days: 90,
    }
}
pub(super) fn message(event: &str, from: &str, to: &str, text: &str) -> Incoming {
    Incoming {
        event: event.into(),
        from: from.into(),
        to: to.into(),
        text: text.into(),
    }
}
pub(super) async fn link(db: &PgStore, person: &User, phone: &str) {
    let code = db.mobile_link_code(person).await.unwrap()["code"]
        .as_str()
        .unwrap()
        .to_owned();
    let event = uuid::Uuid::new_v4().to_string();
    let (outcome, problem) = db
        .whatsapp_incoming(&message(&event, phone, FIRST, &code.to_lowercase()))
        .await
        .unwrap();
    assert_eq!((outcome, problem), (Outcome::Linked, None));
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn whatsapp_numbers_are_given_once_per_agent_and_rest_after_release() {
    let db = fixture().await;
    let owner = user(&db, "wa-owner").await;
    let w = workspace(&db, &owner).await;
    let (_, nova) = super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    let (_, atlas) = super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    let (_, third) = super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    let member = user(&db, "wa-member").await;
    join(&db, &owner, &w, &member, "Member").await;
    db.chat_grant(&owner, &w, &member.id, &nova, true)
        .await
        .unwrap();
    let pool = offer(&[FIRST, SECOND]);

    // Off by default; only the agent's owner or an admin switches it on.
    assert_eq!(
        db.whatsapp_status(&owner, &w, &nova, true).await.unwrap()["state"],
        "off"
    );
    assert!(
        db.whatsapp_activate(&member, &w, &nova, &pool)
            .await
            .is_err()
    );
    let on = db
        .whatsapp_activate(&owner, &w, &nova, &pool)
        .await
        .unwrap();
    assert_eq!(
        (on["state"].as_str(), on["number"].as_str()),
        (Some("active"), Some(FIRST))
    );
    // Repeating it keeps the number; a person who may talk to the agent sees it.
    assert_eq!(
        db.whatsapp_activate(&owner, &w, &nova, &pool)
            .await
            .unwrap()["number"],
        FIRST
    );
    let seen = db.whatsapp_status(&member, &w, &nova, true).await.unwrap();
    assert_eq!(
        (seen["number"].as_str(), seen["can_manage"].as_bool()),
        (Some(FIRST), Some(false))
    );
    assert!(db.whatsapp_status(&member, &w, &atlas, true).await.is_err());

    assert_eq!(
        db.whatsapp_activate(&owner, &w, &atlas, &pool)
            .await
            .unwrap()["number"],
        SECOND
    );
    let none = db
        .whatsapp_activate(&owner, &w, &third, &pool)
        .await
        .unwrap_err();
    assert!(none.is::<NoFreeNumber>());

    // Released numbers rest; the same agent gets its own back, others don't.
    assert_eq!(
        db.whatsapp_deactivate(&owner, &w, &nova, &pool)
            .await
            .unwrap()["state"],
        "off"
    );
    assert!(
        db.whatsapp_activate(&owner, &w, &third, &pool)
            .await
            .unwrap_err()
            .is::<NoFreeNumber>()
    );
    let back = db
        .whatsapp_activate(&owner, &w, &nova, &pool)
        .await
        .unwrap();
    assert_eq!(back["number"], FIRST);
    let revision: i64 = query(&db.db, "SELECT e.revision FROM relay_communication_endpoints e JOIN relay_whatsapp_numbers n ON n.endpoint=e.id WHERE n.number=$1", vec![FIRST.into()])
        .await.unwrap().unwrap().try_get("", "revision").unwrap();
    assert_eq!(revision, 3, "each switch is a new generation");

    // After resting, a number never goes to another agent of the same
    // workspace, but another workspace can have it.
    db.whatsapp_deactivate(&owner, &w, &atlas, &pool)
        .await
        .unwrap();
    execute(
        &db.db,
        "UPDATE relay_whatsapp_numbers SET ready_after=0 WHERE number=$1",
        vec![SECOND.into()],
    )
    .await
    .unwrap();
    assert!(
        db.whatsapp_activate(&owner, &w, &third, &pool)
            .await
            .unwrap_err()
            .is::<NoFreeNumber>()
    );
    let other = user(&db, "wa-other-owner").await;
    let other_w = workspace(&db, &other).await;
    let (_, theirs) = super::super::executor_slot_tests::agent(&db, &other, &other_w).await;
    assert_eq!(
        db.whatsapp_activate(&other, &other_w, &theirs, &pool)
            .await
            .unwrap()["number"],
        SECOND
    );
    // A number removed from the operator's file is not given out.
    db.whatsapp_deactivate(&other, &other_w, &theirs, &pool)
        .await
        .unwrap();
    assert!(
        db.whatsapp_activate(&other, &other_w, &theirs, &offer(&[FIRST]))
            .await
            .unwrap_err()
            .is::<NoFreeNumber>()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn whatsapp_messages_are_routed_by_the_senders_linked_number() {
    let db = fixture().await;
    let owner = user(&db, "wa-route-owner").await;
    let w = workspace(&db, &owner).await;
    let (_, nova) = super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    let member = user(&db, "wa-route-member").await;
    join(&db, &owner, &w, &member, "Member").await;
    db.whatsapp_activate(&owner, &w, &nova, &offer(&[FIRST]))
        .await
        .unwrap();

    // Nobody has linked a number yet: held, with a fixed reply and no work.
    let (outcome, _) = db
        .whatsapp_incoming(&message("SM1", OWNER_PHONE, FIRST, "Hello"))
        .await
        .unwrap();
    assert_eq!(outcome, Outcome::Held);
    assert!(outcome.reply(None).is_some());
    assert!(db.claim_job().await.unwrap().is_none());

    // Bad and reused codes are refused with a fixed reply.
    let (outcome, problem) = db
        .whatsapp_incoming(&message("SM2", OWNER_PHONE, FIRST, "LINK-ABC234"))
        .await
        .unwrap();
    assert_eq!(outcome, Outcome::Ignored);
    assert!(problem.is_some());
    link(&db, &owner, OWNER_PHONE).await;
    assert_eq!(
        db.mobile_status(&owner).await.unwrap()["number"],
        OWNER_PHONE
    );
    let code = db.mobile_link_code(&member).await.unwrap()["code"]
        .as_str()
        .unwrap()
        .to_owned();
    let (_, taken) = db
        .whatsapp_incoming(&message("SM3", OWNER_PHONE, FIRST, &code))
        .await
        .unwrap();
    assert!(taken.is_some(), "a number belongs to one person");

    // The owner's message reaches the agent in their direct room with it.
    let (outcome, _) = db
        .whatsapp_incoming(&message(
            "SM4",
            OWNER_PHONE,
            FIRST,
            "OWNER_PRIVATE_QUESTION",
        ))
        .await
        .unwrap();
    assert_eq!(outcome, Outcome::Admitted);
    let (again, _) = db
        .whatsapp_incoming(&message(
            "SM4",
            OWNER_PHONE,
            FIRST,
            "OWNER_PRIVATE_QUESTION",
        ))
        .await
        .unwrap();
    assert_eq!(again, Outcome::Duplicate);
    let job = db.claim_job().await.unwrap().unwrap();
    assert_eq!(job.requester, owner.id);
    let owner_room = job.chat.clone();
    db.chat_finish(&job, "Your answer", "complete")
        .await
        .unwrap();
    let out = json_rows(
        &db.db,
        "SELECT to_jsonb(o) AS data FROM relay_communication_outbox o WHERE workspace=$1 AND id=$2",
        vec![w.clone().into(), job.reply.clone().into()],
    )
    .await
    .unwrap()
    .remove(0);
    assert_eq!(
        (out["state"].as_str(), out["envelope"]["recipient"].as_str()),
        (Some("queued"), Some(OWNER_PHONE))
    );

    // The member links a number but has no access to Nova: held.
    link(&db, &member, MEMBER_PHONE).await;
    let (outcome, _) = db
        .whatsapp_incoming(&message("SM5", MEMBER_PHONE, FIRST, "Let me in"))
        .await
        .unwrap();
    assert_eq!(outcome, Outcome::Held);
    db.chat_grant(&owner, &w, &member.id, &nova, true)
        .await
        .unwrap();
    let (outcome, _) = db
        .whatsapp_incoming(&message("SM6", MEMBER_PHONE, FIRST, "MEMBER_QUESTION"))
        .await
        .unwrap();
    assert_eq!(outcome, Outcome::Admitted);
    let job = db.claim_job().await.unwrap().unwrap();
    assert_eq!(job.requester, member.id);
    assert_ne!(job.chat, owner_room, "two people never share a room");
    db.chat_finish(&job, "Member answer", "complete")
        .await
        .unwrap();

    // Unlinking stops the door and keeps the room: linking again comes back
    // to the person's same direct conversation (E3).
    db.mobile_unlink(&owner).await.unwrap();
    let (outcome, _) = db
        .whatsapp_incoming(&message("SM7", OWNER_PHONE, FIRST, "Still me"))
        .await
        .unwrap();
    assert_eq!(outcome, Outcome::Held);
    let cancelled: String = query(&db.db, "SELECT state FROM relay_communication_outbox WHERE workspace=$1 AND envelope->>'recipient'=$2", vec![w.clone().into(), OWNER_PHONE.into()])
        .await.unwrap().unwrap().try_get("", "state").unwrap();
    assert_eq!(
        cancelled, "cancelled",
        "an unsent reply to an unlinked number is not sent"
    );
    link(&db, &owner, OWNER_PHONE).await;
    db.whatsapp_incoming(&message("SM8", OWNER_PHONE, FIRST, "Back again"))
        .await
        .unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    assert_eq!(job.chat, owner_room);

    // Admins can't bind WhatsApp senders by hand.
    let endpoint: String = query(
        &db.db,
        "SELECT endpoint FROM relay_whatsapp_numbers WHERE number=$1",
        vec![FIRST.into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "endpoint")
    .unwrap();
    let binding = crate::identity::communications::model::Binding {
        sender: "+919800000009".into(),
        user: member.id.clone(),
    };
    assert!(
        db.communication_binding(&owner, &w, &endpoint, &binding)
            .await
            .is_err()
    );
}

/// Each request: its Authorization header and form fields.
type Requests = Arc<Mutex<Vec<(String, Vec<(String, String)>)>>>;
/// A local stand-in for Twilio's Messages API: records each form it was sent.
#[derive(Clone, Default)]
struct FakeTwilio {
    sent: Requests,
    refuse: Arc<Mutex<bool>>,
}
async fn messages(
    State(fake): State<FakeTwilio>,
    headers: HeaderMap,
    Form(form): Form<Vec<(String, String)>>,
) -> (axum::http::StatusCode, String) {
    let auth = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    fake.sent.lock().unwrap().push((auth, form));
    if *fake.refuse.lock().unwrap() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            r#"{"code":63016,"message":"Outside the allowed window"}"#.into(),
        );
    }
    (
        axum::http::StatusCode::CREATED,
        r#"{"sid":"SMfake1"}"#.into(),
    )
}
fn private(dir: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, body).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    path
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn whatsapp_replies_go_through_twilio_to_the_rooms_number_only() {
    let db = fixture().await;
    let owner = user(&db, "wa-send-owner").await;
    let w = workspace(&db, &owner).await;
    let (_, nova) = super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    db.whatsapp_activate(&owner, &w, &nova, &offer(&[FIRST]))
        .await
        .unwrap();
    link(&db, &owner, OWNER_PHONE).await;

    let fake = FakeTwilio::default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api = format!("http://{}", listener.local_addr().unwrap());
    let app = Router::new()
        .route(
            &format!("/2010-04-01/Accounts/{}/Messages.json", account()),
            post(messages),
        )
        .with_state(fake.clone());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let dir = tempfile::tempdir().unwrap();
    let token = private(dir.path(), "token", "fixture-twilio-token-0001\n");
    let file = private(
        dir.path(),
        "whatsapp.json",
        &serde_json::json!({
            "account": account(), "auth_token_file": token, "api": api, "numbers": [FIRST]
        })
        .to_string(),
    );
    let adapters: Vec<Arc<dyn TransportAdapter>> = vec![Arc::new(WhatsappTransport::with_file(
        file,
        "https://cuckoo.example.com",
    ))];

    db.whatsapp_incoming(&message(
        "SM10",
        OWNER_PHONE,
        FIRST,
        "What is my leave balance?",
    ))
    .await
    .unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    db.chat_finish(&job, "You have **12 days** left.", "complete")
        .await
        .unwrap();
    let round = gateway::run_once(&db, &adapters).await;
    assert_eq!(round.sent, 1);
    let sent = fake.sent.lock().unwrap().clone();
    assert_eq!(sent.len(), 1);
    let (auth, form) = &sent[0];
    assert!(auth.starts_with("Basic "));
    let field = |name: &str| {
        form.iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    };
    assert_eq!(field("From"), Some("whatsapp:+14155550101"));
    assert_eq!(field("To"), Some("whatsapp:+919800000001"));
    assert_eq!(field("Body"), Some("You have *12 days* left."));
    let out = json_rows(
        &db.db,
        "SELECT to_jsonb(o) AS data FROM relay_communication_outbox o WHERE workspace=$1 AND id=$2",
        vec![w.clone().into(), job.reply.clone().into()],
    )
    .await
    .unwrap()
    .remove(0);
    assert_eq!(
        (out["state"].as_str(), out["provider_id"].as_str()),
        (Some("accepted"), Some("SMfake1"))
    );
    let callback = field("StatusCallback").unwrap();
    assert!(callback.starts_with("https://cuckoo.example.com/hooks/whatsapp/twilio/status?"));
    assert!(callback.contains(out["attempt"].as_str().unwrap()));

    // A delivery update settles it and never downgrades a delivered reply.
    db.whatsapp_receipt(&w, out["attempt"].as_str().unwrap(), true)
        .await
        .unwrap();
    db.whatsapp_receipt(&w, out["attempt"].as_str().unwrap(), false)
        .await
        .unwrap();
    let state: String = query(
        &db.db,
        "SELECT state FROM relay_communication_outbox WHERE workspace=$1 AND id=$2",
        vec![w.clone().into(), job.reply.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "state")
    .unwrap();
    assert_eq!(state, "delivered");

    // Twilio refusing (outside the 24-hour window) is a failed send with its reason.
    *fake.refuse.lock().unwrap() = true;
    db.whatsapp_incoming(&message("SM11", OWNER_PHONE, FIRST, "Another question"))
        .await
        .unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    db.chat_finish(&job, "Late answer", "complete")
        .await
        .unwrap();
    gateway::run_once(&db, &adapters).await;
    let out = json_rows(
        &db.db,
        "SELECT to_jsonb(o) AS data FROM relay_communication_outbox o WHERE workspace=$1 AND id=$2",
        vec![w.clone().into(), job.reply.clone().into()],
    )
    .await
    .unwrap()
    .remove(0);
    assert_eq!(out["state"], "failed");
    assert!(out["reason"].as_str().unwrap().contains("63016"));
}

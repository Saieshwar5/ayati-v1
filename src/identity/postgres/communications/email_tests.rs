//! Email on Cuckoo's mail domain end to end against a local fake Postmark:
//! switching an agent's address on and off, who may write, one room per
//! thread, mail that is never answered, sending the reply and delivery events.
use super::super::{tests::*, *};
use super::email_people::Outcome;
use crate::identity::communications::{
    adapters::TransportAdapter,
    email::{inbound::Parsed, postmark::EmailTransport},
    gateway,
};
use axum::{Json, Router, extract::State, http::HeaderMap, routing::post};
use serde_json::{Value, json};
use std::{
    os::unix::fs::PermissionsExt,
    sync::{Arc, Mutex},
};

const DOMAIN: &str = "agents.cuckoo.test";

/// An authenticated, single-recipient email starting its own thread.
fn mail(event: &str, from: &str, to: &str, text: &str) -> Parsed {
    Parsed {
        event: event.into(),
        from: from.into(),
        to: to.into(),
        others: false,
        authenticated: true,
        automated: false,
        message_id: format!("<{event}@example.test>"),
        root: format!("<{event}@example.test>"),
        references: vec![format!("<{event}@example.test>")],
        subject: "Onboarding".into(),
        text: Some(text.into()),
        attachments: false,
    }
}
/// A reply in the thread `first` started.
fn reply(event: &str, first: &Parsed, text: &str) -> Parsed {
    let mut m = mail(event, &first.from, &first.to, text);
    m.root = first.root.clone();
    m.references = vec![first.root.clone(), m.message_id.clone()];
    m.subject = "Re: Onboarding".into();
    m
}
async fn address(db: &PgStore, owner: &User, w: &str, agent: &str) -> String {
    let on = db.email_activate(owner, w, agent, DOMAIN).await.unwrap();
    assert_eq!(on["state"], "active");
    on["address"].as_str().unwrap().to_owned()
}
async fn outbox(db: &PgStore, w: &str, reply: &str) -> Value {
    json_rows(
        &db.db,
        "SELECT to_jsonb(o) AS data FROM relay_communication_outbox o WHERE workspace=$1 AND id=$2",
        vec![w.into(), reply.into()],
    )
    .await
    .unwrap()
    .remove(0)
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn email_addresses_are_made_once_per_agent_and_never_reused() {
    let db = fixture().await;
    let owner = user(&db, "em-owner").await;
    let w = workspace(&db, &owner).await;
    let (_, nova) = super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    let (_, atlas) = super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    let member = user(&db, "em-member").await;
    join(&db, &owner, &w, &member, "Member").await;
    db.chat_grant(&owner, &w, &member.id, &nova, true)
        .await
        .unwrap();

    // Off by default; only the agent's owner or an admin switches it on.
    let off = db
        .email_status(&owner, &w, &nova, Some(DOMAIN))
        .await
        .unwrap();
    assert_eq!(
        (off["state"].as_str(), off["offered"].as_bool()),
        (Some("off"), Some(true))
    );
    assert!(db.email_activate(&member, &w, &nova, DOMAIN).await.is_err());
    let first = address(&db, &owner, &w, &nova).await;
    assert!(first.ends_with(&format!("@{DOMAIN}")), "{first}");
    // Repeating it keeps the address; a person who may talk to the agent sees it.
    assert_eq!(address(&db, &owner, &w, &nova).await, first);
    let seen = db
        .email_status(&member, &w, &nova, Some(DOMAIN))
        .await
        .unwrap();
    assert_eq!(
        (seen["address"].as_str(), seen["can_manage"].as_bool()),
        (Some(first.as_str()), Some(false))
    );
    assert!(
        db.email_status(&member, &w, &atlas, Some(DOMAIN))
            .await
            .is_err()
    );
    let other = address(&db, &owner, &w, &atlas).await;
    assert_ne!(other, first);

    // Switching off releases the address; the same agent gets it back as a new
    // generation, and no other agent ever has it.
    assert!(
        db.email_deactivate(&member, &w, &nova, Some(DOMAIN))
            .await
            .is_err()
    );
    let off = db
        .email_deactivate(&owner, &w, &nova, Some(DOMAIN))
        .await
        .unwrap();
    assert_eq!(off["state"], "off");
    assert_eq!(address(&db, &owner, &w, &nova).await, first);
    let revision: i64 = query(&db.db, "SELECT e.revision FROM relay_communication_endpoints e JOIN relay_email_addresses a ON a.endpoint=e.id WHERE a.address=$1", vec![first.clone().into()])
        .await.unwrap().unwrap().try_get("", "revision").unwrap();
    assert_eq!(revision, 3, "each switch is a new generation");
    // A server that stops offering the domain shows the address as paused.
    let paused = db
        .email_status(&owner, &w, &nova, Some("other.test"))
        .await
        .unwrap();
    assert_eq!(paused["state"], "paused");

    // Neither the generic endpoint nor binding APIs can touch email endpoints.
    let endpoint: String = query(
        &db.db,
        "SELECT endpoint FROM relay_email_addresses WHERE address=$1",
        vec![first.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "endpoint")
    .unwrap();
    let binding = crate::identity::communications::model::Binding {
        sender: "someone@example.test".into(),
        user: member.id.clone(),
    };
    assert!(
        db.communication_binding(&owner, &w, &endpoint, &binding)
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn email_is_read_only_from_members_whose_domain_vouched_for_it() {
    let db = fixture().await;
    let owner = user(&db, "em-route-owner").await;
    let w = workspace(&db, &owner).await;
    let (_, nova) = super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    let member = user(&db, "em-route-member").await;
    join(&db, &owner, &w, &member, "Member").await;
    let to = address(&db, &owner, &w, &nova).await;
    let owner_mail = "em-route-owner@example.test";
    let member_mail = "em-route-member@example.test";

    // A stranger, and mail to an address nobody has, get no work.
    let stranger = mail("pm-1", "stranger@example.test", &to, "Hello");
    assert_eq!(db.email_incoming(&stranger).await.unwrap(), Outcome::Held);
    let nowhere = mail("pm-2", owner_mail, &format!("nobody-x@{DOMAIN}"), "Hi");
    assert_eq!(db.email_incoming(&nowhere).await.unwrap(), Outcome::Ignored);

    // Forged, automated, shared or unreadable mail from the owner is never answered.
    for (event, change) in [
        (
            "pm-3",
            (|m: &mut Parsed| m.authenticated = false) as fn(&mut Parsed),
        ),
        ("pm-4", |m: &mut Parsed| m.automated = true),
        ("pm-5", |m: &mut Parsed| m.others = true),
        ("pm-6", |m: &mut Parsed| m.text = None),
    ] {
        let mut m = mail(event, owner_mail, &to, "SHOULD_NOT_BE_KEPT");
        change(&mut m);
        assert_eq!(
            db.email_incoming(&m).await.unwrap(),
            Outcome::Ignored,
            "{event}"
        );
    }
    assert!(db.claim_job().await.unwrap().is_none());
    let kept: i64 = query(&db.db, "SELECT count(*) AS total FROM relay_communication_inbox WHERE workspace=$1 AND envelope::text LIKE '%SHOULD_NOT_BE_KEPT%'", vec![w.clone().into()])
        .await.unwrap().unwrap().try_get("", "total").unwrap();
    assert_eq!(kept, 0, "unread mail bodies are not kept");

    // The owner's mail reaches the agent; a retry is recognised.
    let first = mail("pm-7", owner_mail, &to, "OWNER_PRIVATE_QUESTION");
    assert_eq!(db.email_incoming(&first).await.unwrap(), Outcome::Admitted);
    assert_eq!(db.email_incoming(&first).await.unwrap(), Outcome::Duplicate);
    let job = db.claim_job().await.unwrap().unwrap();
    assert_eq!(job.requester, owner.id);
    let room = job.chat.clone();
    db.chat_finish(&job, "Your answer", "complete")
        .await
        .unwrap();
    let out = outbox(&db, &w, &job.reply).await;
    assert_eq!(
        (out["state"].as_str(), out["envelope"]["recipient"].as_str()),
        (Some("queued"), Some(owner_mail))
    );

    // A reply in the same thread stays in the room; a new email opens another.
    db.email_incoming(&reply("pm-8", &first, "Follow-up"))
        .await
        .unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    assert_eq!(job.chat, room);
    db.chat_finish(&job, "Follow-up answer", "complete")
        .await
        .unwrap();
    db.email_incoming(&mail("pm-9", owner_mail, &to, "New topic"))
        .await
        .unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    assert_ne!(job.chat, room, "one room per thread");
    db.chat_finish(&job, "New answer", "complete")
        .await
        .unwrap();

    // A member without access to the agent is held until granted, and gets a
    // room of their own.
    let asked = mail("pm-10", member_mail, &to, "Let me in");
    assert_eq!(db.email_incoming(&asked).await.unwrap(), Outcome::Held);
    db.chat_grant(&owner, &w, &member.id, &nova, true)
        .await
        .unwrap();
    let asked = mail("pm-11", member_mail, &to, "MEMBER_QUESTION");
    assert_eq!(db.email_incoming(&asked).await.unwrap(), Outcome::Admitted);
    let job = db.claim_job().await.unwrap().unwrap();
    assert_eq!(job.requester, member.id);
    assert_ne!(job.chat, room, "two people never share a room");

    // Switching email off stops reading and cancels the unsent reply.
    db.email_deactivate(&owner, &w, &nova, Some(DOMAIN))
        .await
        .unwrap();
    // The running work lost its room, so finishing it may be refused.
    let _ = db.chat_finish(&job, "Member answer", "complete").await;
    let late = mail("pm-12", owner_mail, &to, "Anyone there?");
    assert_eq!(db.email_incoming(&late).await.unwrap(), Outcome::Ignored);
    let ready = db.claim_deliveries(10).await.unwrap();
    assert!(ready.is_empty(), "no reply goes out after switching off");
}

/// Each request: its server token and JSON body.
type Requests = Arc<Mutex<Vec<(String, Value)>>>;
/// A local stand-in for Postmark's Email API: records each message it was sent.
#[derive(Clone, Default)]
struct FakePostmark {
    sent: Requests,
    refuse: Arc<Mutex<bool>>,
}
async fn send_email(
    State(fake): State<FakePostmark>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> (axum::http::StatusCode, Json<Value>) {
    let token = headers
        .get("x-postmark-server-token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    fake.sent.lock().unwrap().push((token, body));
    if *fake.refuse.lock().unwrap() {
        return (
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            Json(
                json!({"ErrorCode": 406, "Message": "You tried to send to a recipient that has been marked as inactive."}),
            ),
        );
    }
    (
        axum::http::StatusCode::OK,
        Json(json!({"ErrorCode": 0, "Message": "OK", "MessageID": "pm-out-1"})),
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
async fn email_replies_go_through_postmark_in_the_same_thread() {
    let db = fixture().await;
    let owner = user(&db, "em-send-owner").await;
    let w = workspace(&db, &owner).await;
    let (_, nova) = super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    let to = address(&db, &owner, &w, &nova).await;
    let owner_mail = "em-send-owner@example.test";

    let fake = FakePostmark::default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api = format!("http://{}", listener.local_addr().unwrap());
    let app = Router::new()
        .route("/email", post(send_email))
        .with_state(fake.clone());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let dir = tempfile::tempdir().unwrap();
    let token = private(dir.path(), "token", "fixture-postmark-token-0001\n");
    let password = private(dir.path(), "password", "fixture-webhook-password-01\n");
    let file = private(
        dir.path(),
        "email.json",
        &json!({"domain": DOMAIN, "server_token_file": token, "webhook_user": "postmark",
            "webhook_password_file": password, "api": api})
        .to_string(),
    );
    let adapters: Vec<Arc<dyn TransportAdapter>> = vec![Arc::new(EmailTransport::with_file(file))];

    let first = mail("pm-20", owner_mail, &to, "What is my leave balance?");
    db.email_incoming(&first).await.unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    db.chat_finish(&job, "You have **12 days** left.", "complete")
        .await
        .unwrap();
    let round = gateway::run_once(&db, &adapters).await;
    assert_eq!(round.sent, 1);
    let sent = fake.sent.lock().unwrap().clone();
    assert_eq!(sent.len(), 1);
    let (token, body) = &sent[0];
    assert_eq!(token, "fixture-postmark-token-0001");
    assert_eq!(body["To"], owner_mail);
    assert!(body["From"].as_str().unwrap().ends_with(&format!("<{to}>")));
    assert_eq!(body["Subject"], "Re: Onboarding");
    assert_eq!(body["TextBody"], "You have **12 days** left.");
    let header = |name: &str| {
        body["Headers"]
            .as_array()
            .unwrap()
            .iter()
            .find(|h| h["Name"] == name)
            .map(|h| h["Value"].as_str().unwrap().to_owned())
    };
    assert_eq!(
        header("In-Reply-To").as_deref(),
        Some("<pm-20@example.test>")
    );
    assert_eq!(
        header("References").as_deref(),
        Some("<pm-20@example.test>")
    );
    assert_eq!(header("Auto-Submitted").as_deref(), Some("auto-replied"));
    let out = outbox(&db, &w, &job.reply).await;
    assert_eq!(
        (out["state"].as_str(), out["provider_id"].as_str()),
        (Some("accepted"), Some("pm-out-1"))
    );
    assert_eq!(body["Metadata"]["cuckoo_key"], out["attempt"]);
    assert_eq!(body["Metadata"]["cuckoo_workspace"], w.as_str());

    // A delivery event settles it and a later bounce never downgrades it.
    let attempt = out["attempt"].as_str().unwrap();
    db.email_receipt(&w, attempt, true).await.unwrap();
    db.email_receipt(&w, attempt, false).await.unwrap();
    assert_eq!(outbox(&db, &w, &job.reply).await["state"], "delivered");

    // A reply later in the thread carries the whole chain.
    db.email_incoming(&reply("pm-21", &first, "And next year?"))
        .await
        .unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    db.chat_finish(&job, "Also 12.", "complete").await.unwrap();
    gateway::run_once(&db, &adapters).await;
    let body = fake.sent.lock().unwrap()[1].1.clone();
    let refs = body["Headers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["Name"] == "References")
        .unwrap()["Value"]
        .clone();
    assert_eq!(refs, "<pm-20@example.test> <pm-21@example.test>");
    assert_eq!(body["Subject"], "Re: Onboarding");

    // Postmark refusing is a failed send with its reason, never resent.
    *fake.refuse.lock().unwrap() = true;
    db.email_incoming(&mail("pm-22", owner_mail, &to, "Another question"))
        .await
        .unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    db.chat_finish(&job, "Late answer", "complete")
        .await
        .unwrap();
    gateway::run_once(&db, &adapters).await;
    let out = outbox(&db, &w, &job.reply).await;
    assert_eq!(out["state"], "failed");
    assert!(out["reason"].as_str().unwrap().contains("406"));
}

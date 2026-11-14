//! Agent email end to end against local stand-ins for SES, SQS and S3: an
//! agent's address switched on and off, mail from a member answered in the
//! same mail thread and the same direct room, forged and outside senders held,
//! automatic mail ignored, and delivery events settling the reply.
use super::super::{tests::*, *};
use super::email_people::{Incoming, Outcome};
use crate::identity::communications::{
    adapters::TransportAdapter,
    email::{inbound, notification::tests::received_body, queue, ses::EmailTransport},
    gateway,
};
use axum::{Json, Router, extract::State, http::HeaderMap, routing::post};
use object_store::{ObjectStoreExt, memory::InMemory, path::Path as ObjectPath};
use serde_json::{Value, json};
use std::{
    os::unix::fs::PermissionsExt,
    sync::{Arc, Mutex},
};

const DOMAIN: &str = "mail.test.example";

fn mail(from: &str, subject: &str, text: &str, parents: &[&str]) -> inbound::Mail {
    inbound::Mail {
        message_id: Some(format!("{}@acme.example", uuid::Uuid::new_v4())),
        from: Some(from.into()),
        subject: subject.into(),
        text: text.into(),
        parents: parents.iter().map(|p| p.to_string()).collect(),
        automated: false,
        attachments: 0,
    }
}
async fn arrive(db: &PgStore, event: &str, to: &str, dmarc: bool, mail: &inbound::Mail) -> Outcome {
    db.email_incoming(&Incoming {
        event: event.into(),
        to: to.into(),
        dmarc,
        mail,
    })
    .await
    .unwrap()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn email_addresses_are_given_once_and_kept_for_their_agent() {
    let db = fixture().await;
    let owner = user(&db, "mail-owner").await;
    let w = workspace(&db, &owner).await;
    let (_, nova) = super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    let (_, twin) = super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    let member = user(&db, "mail-member").await;
    join(&db, &owner, &w, &member, "Member").await;

    assert_eq!(
        db.email_status(&owner, &w, &nova, true).await.unwrap()["state"],
        "off"
    );
    assert!(db.email_activate(&member, &w, &nova, DOMAIN).await.is_err());
    let on = db.email_activate(&owner, &w, &nova, DOMAIN).await.unwrap();
    let address = on["address"].as_str().unwrap().to_owned();
    assert_eq!(address, format!("pool-agent.test-business@{DOMAIN}"));
    assert_eq!(
        db.email_activate(&owner, &w, &nova, DOMAIN).await.unwrap()["address"],
        address.as_str()
    );
    // A second agent with the same name gets its own address.
    assert_eq!(
        db.email_activate(&owner, &w, &twin, DOMAIN).await.unwrap()["address"],
        format!("pool-agent-2.test-business@{DOMAIN}").as_str()
    );
    // People who may talk to the agent see the address; others don't.
    assert!(db.email_status(&member, &w, &nova, true).await.is_err());
    db.chat_grant(&owner, &w, &member.id, &nova, true)
        .await
        .unwrap();
    let seen = db.email_status(&member, &w, &nova, true).await.unwrap();
    assert_eq!(
        (seen["address"].as_str(), seen["can_manage"].as_bool()),
        (Some(address.as_str()), Some(false))
    );

    // Off keeps the address for this agent; on again brings it back.
    let off = db.email_deactivate(&owner, &w, &nova).await.unwrap();
    assert_eq!(
        (off["state"].as_str(), off["address"].as_str()),
        (Some("off"), Some(address.as_str()))
    );
    assert_eq!(
        db.email_activate(&owner, &w, &nova, DOMAIN).await.unwrap()["address"],
        address.as_str()
    );
    // The generic admin APIs can't bind email senders by hand.
    let endpoint: String = query(
        &db.db,
        "SELECT endpoint FROM relay_email_addresses WHERE address=$1",
        vec![address.into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "endpoint")
    .unwrap();
    let binding = crate::identity::communications::model::Binding {
        sender: "someone@acme.example".into(),
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
async fn only_authenticated_mail_from_members_reaches_the_agent() {
    let db = fixture().await;
    let owner = user(&db, "mail-route-owner").await;
    let w = workspace(&db, &owner).await;
    let (_, nova) = super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    let address = db.email_activate(&owner, &w, &nova, DOMAIN).await.unwrap()["address"]
        .as_str()
        .unwrap()
        .to_owned();
    let me = owner.email.clone();

    // A forged From (DMARC did not pass) and an outsider are held, without work.
    assert_eq!(
        arrive(
            &db,
            "ses-f",
            &address,
            false,
            &mail(&me, "Hi", "Forged", &[])
        )
        .await,
        Outcome::Held
    );
    assert_eq!(
        arrive(
            &db,
            "ses-o",
            &address,
            true,
            &mail("stranger@else.example", "Hi", "Outside", &[])
        )
        .await,
        Outcome::Held
    );
    let mut auto = mail(&me, "Out of office", "Away", &[]);
    auto.automated = true;
    assert_eq!(
        arrive(&db, "ses-a", &address, true, &auto).await,
        Outcome::Ignored
    );
    assert_eq!(
        arrive(
            &db,
            "ses-u",
            &format!("nobody@{DOMAIN}"),
            true,
            &mail(&me, "Hi", "x", &[])
        )
        .await,
        Outcome::Ignored
    );
    assert!(db.claim_job().await.unwrap().is_none());
    let held: String = query(
        &db.db,
        "SELECT envelope::text AS e FROM relay_communication_inbox WHERE event='ses-f'",
        vec![],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "e")
    .unwrap();
    assert!(!held.contains("Forged"), "a held mail's text is not kept");

    // The member's own authenticated mail reaches the agent in their direct room,
    // also when written to a +tag of the address and in capitals.
    let first = mail(
        &me.to_uppercase().to_lowercase(),
        "Leave balance",
        "How many days do I have?",
        &[],
    );
    let tagged = address.replace('@', "+leave@").to_uppercase();
    assert_eq!(
        arrive(&db, "ses-1", &tagged, true, &first).await,
        Outcome::Admitted
    );
    assert_eq!(
        arrive(&db, "ses-1", &tagged, true, &first).await,
        Outcome::Duplicate
    );
    let job = db.claim_job().await.unwrap().unwrap();
    assert_eq!(job.requester, owner.id);
    assert!(job.prompt.contains("Leave balance") && job.prompt.contains("How many days"));
    let kind: String = query(
        &db.db,
        "SELECT kind FROM relay_chats WHERE id=$1",
        vec![job.chat.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "kind")
    .unwrap();
    assert_eq!(kind, "direct");
    db.chat_finish(&job, "You have **12** days.", "complete")
        .await
        .unwrap();
    let out: Value = json_rows(
        &db.db,
        "SELECT to_jsonb(o) AS data FROM relay_communication_outbox o WHERE id=$1",
        vec![job.reply.clone().into()],
    )
    .await
    .unwrap()
    .remove(0);
    assert_eq!(
        (out["state"].as_str(), out["envelope"]["recipient"].as_str()),
        (Some("queued"), Some(me.as_str()))
    );
}

/// Each SES call's JSON body, and the queue's messages.
#[derive(Clone, Default)]
struct FakeAws {
    sent: Arc<Mutex<Vec<Value>>>,
    queue: Arc<Mutex<Vec<String>>>,
    deleted: Arc<Mutex<Vec<String>>>,
}
async fn ses(State(fake): State<FakeAws>, Json(body): Json<Value>) -> Json<Value> {
    let mut sent = fake.sent.lock().unwrap();
    sent.push(body);
    Json(json!({"MessageId": format!("ses-out-{}", sent.len())}))
}
/// SQS's JSON protocol is `application/x-amz-json-1.0`, so the body is read as bytes.
async fn sqs(
    State(fake): State<FakeAws>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Json<Value> {
    let body: Value = serde_json::from_slice(&body).unwrap();
    let target = headers
        .get("x-amz-target")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    assert!(
        headers.get("authorization").is_some(),
        "SQS calls are signed"
    );
    if target.ends_with("ReceiveMessage") {
        let messages: Vec<Value> = fake.queue.lock().unwrap().drain(..).enumerate()
            .map(|(i, b)| json!({"MessageId": i.to_string(), "ReceiptHandle": format!("handle-{i}"), "Body": b}))
            .collect();
        return Json(json!({"Messages": messages}));
    }
    fake.deleted.lock().unwrap().push(
        body["ReceiptHandle"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
    );
    Json(json!({}))
}
fn private(dir: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, body).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    path
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn mail_is_read_from_the_queue_answered_in_its_thread_and_settled_by_events() {
    let db = fixture().await;
    let owner = user(&db, "mail-send-owner").await;
    let w = workspace(&db, &owner).await;
    let (_, nova) = super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    let address = db.email_activate(&owner, &w, &nova, DOMAIN).await.unwrap()["address"]
        .as_str()
        .unwrap()
        .to_owned();

    let fake = FakeAws::default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api = format!("http://{}", listener.local_addr().unwrap());
    let app = Router::new()
        .route("/v2/email/outbound-emails", post(ses))
        .route("/", post(sqs))
        .with_state(fake.clone());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let dir = tempfile::tempdir().unwrap();
    let file = private(dir.path(), "email.json", &json!({
        "domain": DOMAIN, "region": "ap-south-1", "queue_url": format!("{api}/1/ayati-mail"),
        "bucket": "ayati-mail", "configuration_set": "ayati-mail", "api": {"ses": api, "sqs": format!("{api}/")}
    }).to_string());
    let transport = EmailTransport::with_file(file.clone());
    let settings = transport.settings().unwrap().unwrap();
    let bucket = InMemory::new();

    // SES stored the member's mail and notified the queue.
    let raw = format!(
        "From: Mail Owner <{}>\r\nTo: {address}\r\nSubject: Leave balance\r\nMessage-ID: <ask-1@acme.example>\r\n\r\nHow many days do I have?\r\n",
        owner.email
    );
    bucket
        .put(&ObjectPath::from("in/ses-1"), raw.into_bytes().into())
        .await
        .unwrap();
    fake.queue
        .lock()
        .unwrap()
        .push(received_body("ses-1", &address, "PASS", "in/ses-1"));
    assert_eq!(
        queue::read_once(&db, &transport, &settings, &bucket)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        *fake.deleted.lock().unwrap(),
        ["handle-0"],
        "deleted only after it was saved"
    );
    let job = db.claim_job().await.unwrap().unwrap();
    let room = job.chat.clone();
    db.chat_finish(&job, "You have **12** days.", "complete")
        .await
        .unwrap();

    // The reply goes out through SES from the agent's address, in the thread.
    let adapters: Vec<Arc<dyn TransportAdapter>> = vec![Arc::new(EmailTransport::with_file(file))];
    assert_eq!(gateway::run_once(&db, &adapters).await.sent, 1);
    let sent = fake.sent.lock().unwrap()[0].clone();
    assert_eq!(sent["FromEmailAddress"], address.as_str());
    assert_eq!(sent["Destination"]["ToAddresses"], json!([owner.email]));
    assert_eq!(sent["ConfigurationSetName"], "ayati-mail");
    use base64::Engine;
    let mime = String::from_utf8(
        base64::engine::general_purpose::STANDARD
            .decode(sent["Content"]["Raw"]["Data"].as_str().unwrap())
            .unwrap(),
    )
    .unwrap();
    assert!(
        mime.contains("In-Reply-To: <ask-1@acme.example>")
            && mime.contains("Subject: Re: Leave balance")
    );
    assert!(mime.contains("From: \"Pool agent\""));
    let attempt = sent["EmailTags"][1]["Value"].as_str().unwrap().to_owned();

    // SES's delivery event settles the reply.
    fake.queue.lock().unwrap().push(
        json!({"eventType":"Delivery","mail":{"messageId":"ses-out-1",
        "tags":{"workspace":[w],"attempt":[attempt]}}})
        .to_string(),
    );
    queue::read_once(&db, &transport, &settings, &bucket)
        .await
        .unwrap();
    let state: String = query(
        &db.db,
        "SELECT state FROM relay_communication_outbox WHERE id=$1",
        vec![job.reply.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "state")
    .unwrap();
    assert_eq!(state, "delivered");

    // The person answers the reply (their client names SES's ID for it): the
    // mail continues the same room thread, under their first mail.
    let follow = mail(
        &owner.email,
        "Re: Leave balance",
        "And sick days?",
        &["ask-1@acme.example", "ses-out-1@ap-south-1.amazonses.com"],
    );
    assert_eq!(
        arrive(&db, "ses-2", &address, true, &follow).await,
        Outcome::Admitted
    );
    let second = db.claim_job().await.unwrap().unwrap();
    assert_eq!(second.chat, room);
    let thread: Option<String> = query(&db.db, "SELECT m.thread FROM relay_chat_jobs j JOIN relay_chat_messages m ON m.workspace=j.workspace AND m.id=j.request_message WHERE j.reply=$1",
        vec![second.reply.clone().into()]).await.unwrap().unwrap().try_get("", "thread").unwrap();
    let first_message: String = query(
        &db.db,
        "SELECT chat_message FROM relay_email_messages WHERE event='ses-1'",
        vec![],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "chat_message")
    .unwrap();
    assert_eq!(thread, Some(first_message));
    assert!(
        second.prompt.contains("12"),
        "the agent remembers the thread"
    );

    // A new subject without references starts a new thread; a subject alone never joins one.
    let fresh = mail(&owner.email, "Re: Leave balance", "Different question", &[]);
    assert_eq!(
        arrive(&db, "ses-3", &address, true, &fresh).await,
        Outcome::Admitted
    );
    db.chat_finish(&second, "Sick days: 6", "complete")
        .await
        .unwrap();
    let third = db.claim_job().await.unwrap().unwrap();
    let thread: Option<String> = query(&db.db, "SELECT m.thread FROM relay_chat_jobs j JOIN relay_chat_messages m ON m.workspace=j.workspace AND m.id=j.request_message WHERE j.reply=$1",
        vec![third.reply.clone().into()]).await.unwrap().unwrap().try_get("", "thread").unwrap();
    assert_eq!(thread, None);

    // Switching email off stops the unsent reply and holds new mail.
    db.chat_finish(&third, "Answer three", "complete")
        .await
        .unwrap();
    db.email_deactivate(&owner, &w, &nova).await.unwrap();
    let state: String = query(
        &db.db,
        "SELECT state FROM relay_communication_outbox WHERE id=$1",
        vec![third.reply.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "state")
    .unwrap();
    assert_eq!(state, "cancelled");
    assert_eq!(
        arrive(
            &db,
            "ses-4",
            &address,
            true,
            &mail(&owner.email, "Hi", "After", &[])
        )
        .await,
        Outcome::Held
    );
}

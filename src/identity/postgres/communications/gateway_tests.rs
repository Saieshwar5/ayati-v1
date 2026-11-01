//! The gateway end to end against a local Communication MCP server: reading,
//! who may write, rooms, sending, receipts, unclear sends and revocation.
use super::super::{tests::*, *};
use crate::identity::communications::{
    adapters::{Route, TransportAdapter},
    fake_transport::{ADDRESS, FakeTransport, Mode, SERVER},
    gateway::{self, Round},
    mcp_transport::McpTransport,
    model::{Binding, LiveEndpoint, SetState},
};
use serde_json::json;
use std::sync::Arc;

struct Setup {
    db: PgStore,
    owner: User,
    w: String,
    endpoint: LiveEndpoint,
    fake: FakeTransport,
    adapters: Vec<Arc<dyn TransportAdapter>>,
}

async fn setup(name: &str) -> Setup {
    let db = fixture().await;
    let owner = user(&db, name).await;
    let w = workspace(&db, &owner).await;
    let (_, agent) = super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    let fake = FakeTransport::start(&w).await;
    let adapters: Vec<Arc<dyn TransportAdapter>> =
        vec![Arc::new(McpTransport::new(Some(fake.file.clone())))];
    let endpoint = LiveEndpoint {
        id: uuid::Uuid::new_v4().to_string(),
        agent,
        transport: "mcp".into(),
        server: SERVER.into(),
        address: ADDRESS.into(),
    };
    let route = Route {
        workspace: w.clone(),
        server: SERVER.into(),
        address: ADDRESS.into(),
    };
    adapters[0].check(&route).await.unwrap();
    db.communication_live_endpoint(&owner, &w, &endpoint)
        .await
        .unwrap();
    db.communication_binding(
        &owner,
        &w,
        &endpoint.id,
        &Binding {
            sender: "U-owner".into(),
            user: owner.id.clone(),
        },
    )
    .await
    .unwrap();
    Setup {
        db,
        owner,
        w,
        endpoint,
        fake,
        adapters,
    }
}

impl Setup {
    async fn round(&self) -> Round {
        gateway::run_once(&self.db, &self.adapters).await
    }
    /// The agent answers the oldest waiting request.
    async fn answer(&self, text: &str) -> String {
        let job = self.db.claim_job().await.unwrap().unwrap();
        self.db.chat_finish(&job, text, "complete").await.unwrap();
        job.reply
    }
    async fn outbox(&self, reply: &str) -> Value {
        json_rows(&self.db.db, "SELECT to_jsonb(o) AS data FROM relay_communication_outbox o WHERE workspace=$1 AND id=$2", vec![self.w.clone().into(), reply.into()])
            .await
            .unwrap()
            .remove(0)
    }
    async fn sql(&self, sql: &str) {
        execute(&self.db.db, sql, vec![]).await.unwrap();
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn communication_gateway_reads_a_live_transport_and_answers_in_the_same_room() {
    let s = setup("gateway-owner").await;
    // An exact retry is fine; the same address for another endpoint is not.
    s.db.communication_live_endpoint(&s.owner, &s.w, &s.endpoint)
        .await
        .unwrap();
    let copy = LiveEndpoint {
        id: uuid::Uuid::new_v4().to_string(),
        agent: s.endpoint.agent.clone(),
        transport: "mcp".into(),
        server: SERVER.into(),
        address: ADDRESS.into(),
    };
    assert!(
        s.db.communication_live_endpoint(&s.owner, &s.w, &copy)
            .await
            .is_err()
    );
    let member = user(&s.db, "gateway-member").await;
    assert!(
        s.db.communication_live_endpoint(&member, &s.w, &copy)
            .await
            .is_err()
    );

    s.fake.message("m1", "U-owner", "PRIVATE_LEAVE_QUESTION");
    s.fake.message("m1", "U-owner", "PRIVATE_LEAVE_QUESTION");
    s.fake.message("m2", "U-stranger", "Let me in");
    s.fake.message("m1", "U-owner", "A changed retry");
    s.fake.push(json!({"type":"message","id":"m3","endpoint":"not-our-bot","sender":{"address":"U-owner"},"conversation":{"id":"D-x","kind":"direct"},"text":"Elsewhere"}));
    let round = s.round().await;
    assert_eq!(
        (round.received, round.skipped),
        (2, 2),
        "changed retry and unknown address are dropped"
    );
    let inbox = json_rows(&s.db.db, "SELECT jsonb_build_object('event',event,'status',status) AS data FROM relay_communication_inbox WHERE workspace=$1 AND transport='mcp' ORDER BY sequence", vec![s.w.clone().into()]).await.unwrap();
    assert_eq!(
        inbox,
        vec![
            json!({"event":"m1","status":"admitted"}),
            json!({"event":"m2","status":"held"})
        ]
    );
    let cursor = query(
        &s.db.db,
        "SELECT cursor FROM relay_communication_cursors WHERE workspace=$1",
        vec![s.w.clone().into()],
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(cursor.try_get::<String>("", "cursor").unwrap(), "5");
    assert_eq!(s.round().await, Round::default(), "nothing is read twice");

    let reply = s.answer("Your leave balance is 12 days").await;
    let room = s.outbox(&reply).await;
    assert_eq!(room["state"], "queued");
    let round = s.round().await;
    assert_eq!(round.sent, 1);
    let sent = s.fake.sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(
        (
            sent[0]["endpoint"].as_str(),
            sent[0]["to"].as_str(),
            sent[0]["conversation"].as_str()
        ),
        (Some(ADDRESS), Some("U-owner"), Some("D-U-owner"))
    );
    assert_eq!(sent[0]["text"], "Your leave balance is 12 days");
    let row = s.outbox(&reply).await;
    assert_eq!(
        (row["state"].as_str(), row["provider_id"].as_str()),
        (Some("accepted"), Some("m-1"))
    );
    assert_eq!(sent[0]["key"], row["attempt"]);

    // The platform confirms delivery later; a failure receipt cannot downgrade it.
    let key = row["attempt"].as_str().unwrap().to_owned();
    s.fake
        .push(json!({"type":"receipt","id":"r1","key":key,"state":"delivered"}));
    s.fake
        .push(json!({"type":"receipt","id":"r2","key":key,"state":"failed"}));
    s.round().await;
    assert_eq!(s.outbox(&reply).await["state"], "delivered");

    // The same sender writes again: same room, so the same private conversation.
    s.fake.message("m4", "U-owner", "And next month?");
    s.round().await;
    let chats = query(&s.db.db, "SELECT count(DISTINCT chat) AS rooms FROM relay_communication_inbox WHERE workspace=$1 AND transport='mcp' AND chat IS NOT NULL", vec![s.w.clone().into()]).await.unwrap().unwrap();
    assert_eq!(chats.try_get::<i64>("", "rooms").unwrap(), 1);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn communication_gateway_never_resends_an_unclear_message() {
    let s = setup("gateway-failures").await;
    let ask = |id: &str| s.fake.message(id, "U-owner", "Question");
    let lease = |reply: &str| {
        format!("UPDATE relay_communication_outbox SET lease_until=0 WHERE id='{reply}'")
    };

    // Refused by the platform: failed with its reason, nothing sent.
    ask("q1");
    s.round().await;
    let refused = s.answer("Refused answer").await;
    s.fake.mode(Mode::Refuse);
    s.round().await;
    let row = s.outbox(&refused).await;
    assert_eq!(
        (row["state"].as_str(), row["reason"].as_str()),
        (Some("failed"), Some("Blocked by the platform"))
    );

    // Unreachable: still queued, this attempt not counted, then sent once back.
    ask("q2");
    s.fake.mode(Mode::Accept);
    s.round().await;
    let later = s.answer("Answer after an outage").await;
    s.fake.mode(Mode::Down);
    assert_eq!(
        s.round().await.unreachable,
        2,
        "reading and sending both failed"
    );
    let row = s.outbox(&later).await;
    assert_eq!(
        (row["state"].as_str(), row["attempts"].as_i64()),
        (Some("queued"), Some(0))
    );
    s.fake.mode(Mode::LoseAnswer);
    s.sql(&lease(&later)).await;
    s.round().await;
    assert_eq!(s.outbox(&later).await["state"], "uncertain");
    // Asked about later: the platform did send it. Never resent.
    s.fake.mode(Mode::Accept);
    s.sql(&lease(&later)).await;
    assert_eq!(s.round().await.settled, 1);
    assert_eq!(s.outbox(&later).await["state"], "delivered");
    assert_eq!(s.fake.sent().len(), 1);

    // Cuckoo stopped after claiming a send: unclear, asked about, never resent.
    ask("q3");
    s.round().await;
    let stopped = s.answer("Interrupted answer").await;
    assert_eq!(s.db.claim_deliveries(20).await.unwrap().len(), 1);
    s.sql(&lease(&stopped)).await;
    s.round().await;
    let row = s.outbox(&stopped).await;
    assert_eq!(row["state"], "uncertain");
    assert_eq!(s.fake.sent().len(), 1);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn communication_gateway_stops_reading_and_sending_when_an_endpoint_is_switched_off() {
    let s = setup("gateway-revoke").await;
    s.fake.message("a1", "U-owner", "Before switching off");
    s.round().await;
    let reply = s.answer("Waiting answer").await;
    s.fake.mode(Mode::Down);
    s.round().await;
    s.db.communication_state(
        &s.owner,
        &s.w,
        &s.endpoint.id,
        None,
        &SetState {
            revision: 1,
            enabled: false,
        },
    )
    .await
    .unwrap();
    s.fake.mode(Mode::Accept);
    s.sql(&format!(
        "UPDATE relay_communication_outbox SET lease_until=0 WHERE id='{reply}'"
    ))
    .await;
    s.fake.message("a2", "U-owner", "After switching off");
    let round = s.round().await;
    assert_eq!((round.received, round.sent), (0, 0));
    assert_eq!(s.outbox(&reply).await["state"], "cancelled");
    assert!(s.fake.sent().is_empty());
}

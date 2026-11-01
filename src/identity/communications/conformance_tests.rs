//! The suite every transport adapter must pass, against a local stand-in for
//! its platform. A new adapter implements [`TestPlatform`] for its own fake and
//! calls [`suite`]. No database: gateway behaviour is in the PostgreSQL tests.
use super::adapters::{Event, Outgoing, Route, Sent, Source, TransportAdapter};
use super::fake_transport::{ADDRESS, BEARER, FakeTransport, Mode, SERVER};
use super::mcp_transport::McpTransport;
use serde_json::json;
use std::sync::Arc;

const WORKSPACE: &str = "00000000-0000-4000-8000-000000000054";

/// What the suite needs from a platform stand-in.
trait TestPlatform {
    fn message(&self, id: &str, sender: &str, text: &str);
    /// A message in a group conversation, which v1 never admits.
    fn group_message(&self, id: &str, sender: &str);
    fn malformed(&self);
    fn receipt(&self, key: &str, delivered: bool);
    /// (to, conversation, text) of every message the platform sent.
    fn sent(&self) -> Vec<(String, String, String)>;
    fn refuse(&self);
    fn lose_answers(&self);
    fn go_down(&self);
    fn recover(&self);
}

impl TestPlatform for FakeTransport {
    fn message(&self, id: &str, sender: &str, text: &str) {
        FakeTransport::message(self, id, sender, text);
    }
    fn group_message(&self, id: &str, sender: &str) {
        self.push(
            json!({"type":"message","id":id,"endpoint":ADDRESS,"sender":{"address":sender},
            "conversation":{"id":"G-1","kind":"group"},"text":"Group message"}),
        );
    }
    fn malformed(&self) {
        self.push(
            json!({"type":"message","id":"bad","endpoint":ADDRESS,"text":"No sender","extra":true}),
        );
    }
    fn receipt(&self, key: &str, delivered: bool) {
        self.push(json!({"type":"receipt","id":format!("r-{key}"),"key":key,"state":if delivered {"delivered"} else {"failed"}}));
    }
    fn sent(&self) -> Vec<(String, String, String)> {
        FakeTransport::sent(self)
            .iter()
            .map(|s| {
                (
                    s["to"].as_str().unwrap().into(),
                    s["conversation"].as_str().unwrap().into(),
                    s["text"].as_str().unwrap().into(),
                )
            })
            .collect()
    }
    fn refuse(&self) {
        self.mode(Mode::Refuse);
    }
    fn lose_answers(&self) {
        self.mode(Mode::LoseAnswer);
    }
    fn go_down(&self) {
        self.mode(Mode::Down);
    }
    fn recover(&self) {
        self.mode(Mode::Accept);
    }
}

fn outgoing(key: &str, text: &str) -> Outgoing {
    Outgoing {
        key: key.into(),
        to: "U-ravi".into(),
        conversation: "D-U-ravi".into(),
        thread: None,
        text: text.into(),
    }
}

async fn suite(
    adapter: Arc<dyn TransportAdapter>,
    platform: &dyn TestPlatform,
    source: Source,
    route: Route,
) {
    // Checking an address before it is saved.
    adapter.check(&route).await.unwrap();
    let unknown = Route {
        address: "someone-else".into(),
        ..route.clone()
    };
    assert!(adapter.check(&unknown).await.is_err());

    // Reading: direct messages only, oldest first, with a cursor.
    platform.message("e1", "U-ravi", "Hello");
    platform.group_message("e2", "U-ravi");
    platform.malformed();
    platform.message("e3", "U-priya", "Second");
    let batch = adapter.fetch(&source, None).await.unwrap();
    assert_eq!(batch.skipped, 2, "group and malformed events are dropped");
    let messages: Vec<_> = batch
        .events
        .iter()
        .filter_map(|e| match e {
            Event::Message(m) => Some(m),
            _ => None,
        })
        .collect();
    assert_eq!(messages.len(), 2);
    assert_eq!(
        (
            messages[0].event.as_str(),
            messages[0].sender.as_str(),
            messages[0].recipient.as_str()
        ),
        ("e1", "U-ravi", route.address.as_str())
    );
    assert!(
        !serde_json::to_string(messages[0])
            .unwrap()
            .contains("Claimed Name"),
        "display names are not identity"
    );
    let again = adapter.fetch(&source, batch.next.as_deref()).await.unwrap();
    assert!(again.events.is_empty() && again.skipped == 0);

    // Sending: Cuckoo's recipient, conversation and text, exactly.
    assert!(matches!(
        adapter
            .send(&route, &outgoing("k1", "Your balance is 4,200"))
            .await,
        Sent::Accepted(Some(_))
    ));
    assert_eq!(
        platform.sent(),
        vec![(
            "U-ravi".into(),
            "D-U-ravi".into(),
            "Your balance is 4,200".into()
        )]
    );
    platform.receipt("k1", true);
    let receipts = adapter.fetch(&source, again.next.as_deref()).await.unwrap();
    assert!(
        matches!(&receipts.events[..], [Event::Receipt { key, delivered: true }] if key == "k1")
    );

    // Refused: nothing sent.
    platform.refuse();
    assert!(matches!(
        adapter.send(&route, &outgoing("k2", "Refused")).await,
        Sent::Refused(_)
    ));
    assert_eq!(platform.sent().len(), 1);

    // Sent, but the answer was lost: unclear, then settled by asking.
    platform.lose_answers();
    assert!(matches!(
        adapter.send(&route, &outgoing("k3", "Lost answer")).await,
        Sent::Uncertain(_)
    ));
    assert!(matches!(
        adapter.status(&route, "k3").await,
        Sent::Delivered(_)
    ));
    assert!(matches!(
        adapter.status(&route, "never-sent").await,
        Sent::Uncertain(_)
    ));

    // Unreachable: certainly not sent, and reading fails without moving on.
    platform.go_down();
    assert!(matches!(
        adapter.send(&route, &outgoing("k4", "Down")).await,
        Sent::Unreached(_)
    ));
    assert!(adapter.fetch(&source, None).await.is_err());
    platform.recover();
    assert_eq!(platform.sent().len(), 2);
}

#[tokio::test]
async fn communication_mcp_adapter_passes_the_transport_suite() {
    let fake = FakeTransport::start(WORKSPACE).await;
    let adapter: Arc<dyn TransportAdapter> = Arc::new(McpTransport::new(Some(fake.file.clone())));
    let source = Source {
        workspace: WORKSPACE.into(),
        transport: "mcp".into(),
        server: SERVER.into(),
    };
    let route = Route {
        workspace: WORKSPACE.into(),
        server: SERVER.into(),
        address: ADDRESS.into(),
    };
    assert_eq!(adapter.servers(WORKSPACE).unwrap()[0].id, SERVER);
    assert!(adapter.servers("another-workspace").unwrap().is_empty());
    suite(adapter, &fake, source, route).await;
}

#[tokio::test]
async fn communication_mcp_refuses_other_protocols_and_unconfigured_servers() {
    let fake = FakeTransport::start(WORKSPACE).await;
    let adapter = McpTransport::new(Some(fake.file.clone()));
    let route = Route {
        workspace: WORKSPACE.into(),
        server: SERVER.into(),
        address: ADDRESS.into(),
    };
    fake.mode(Mode::OtherVersion);
    let refused = adapter.check(&route).await.unwrap_err().to_string();
    assert!(refused.contains("version 1"), "{refused}");
    assert!(matches!(
        adapter.send(&route, &outgoing("k", "x")).await,
        Sent::Unreached(_)
    ));
    fake.mode(Mode::Accept);
    // A server configured for one workspace cannot serve another.
    let other = Route {
        workspace: "00000000-0000-4000-8000-000000000099".into(),
        ..route.clone()
    };
    assert!(adapter.check(&other).await.is_err());
    assert!(McpTransport::new(None).check(&route).await.is_err());
    // The bearer token never appears in what an adapter reports.
    let reasons = format!(
        "{:?} {:?}",
        adapter.send(&other, &outgoing("k", "x")).await,
        adapter.status(&other, "k").await
    );
    assert!(!reasons.contains(BEARER));
}

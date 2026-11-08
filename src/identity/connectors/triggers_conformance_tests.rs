//! The event half of the provider suite (schema 109): every provider with
//! `capabilities().triggers` subscribes an account to a catalog event,
//! unsubscribes it, and reads only fresh, correctly signed deliveries.
use super::fake_composio::{self, FakeComposio};
use super::fake_mcp::WORKSPACE;
use super::*;

/// What the event suite needs from each provider's fake.
trait TriggerFixture {
    fn provider(&self) -> Arc<dyn ConnectorProvider>;
    fn app(&self) -> &'static str;
    /// A connected, working account's binding.
    async fn connected(&self) -> Value;
    /// The provider's trigger name for the app's first catalog event.
    fn trigger(&self) -> String;
    fn subscribed(&self, subscription: &str) -> bool;
    fn expire(&self, binding: &Value);
    /// A signed delivery of one app event, and the secret that verifies it.
    fn delivery(
        &self,
        binding: &Value,
        subscription: &str,
        id: &str,
        at: i64,
    ) -> (axum::http::HeaderMap, Vec<u8>, String);
}

fn saved<'a>(app: &'a str, binding: &'a Value) -> Saved<'a> {
    Saved {
        workspace: WORKSPACE,
        server: app,
        binding: Some(binding),
        fingerprint: "unused",
    }
}

async fn trigger_conformance(fixture: &impl TriggerFixture) {
    let provider = fixture.provider();
    assert!(provider.capabilities().triggers);
    let binding = fixture.connected().await;
    let saved = saved(fixture.app(), &binding);
    let trigger = fixture.trigger();
    // 1. Subscribe: the provider now watches the account.
    let one = provider
        .subscribe(
            &saved,
            &trigger,
            &json!({}),
            "https://cuckoo.example.test/hooks/x",
        )
        .await
        .unwrap();
    assert!(fixture.subscribed(&one.id));
    // 2. Only catalog events can be subscribed.
    assert!(
        provider
            .subscribe(&saved, "NOT_A_CATALOG_EVENT", &json!({}), "https://x.test/")
            .await
            .is_err()
    );
    // 3. A fresh, correctly signed delivery reads as an app event of that subscription.
    let now = crate::identity::user::now();
    let (headers, body, secret) = fixture.delivery(&binding, &one.id, "msg_conformance_1", now);
    let read = provider
        .parse_delivery(&headers, &body, &secret, now)
        .unwrap();
    let [delivery] = read.as_slice() else {
        panic!("one delivery");
    };
    assert_eq!(delivery.event_id, "msg_conformance_1");
    let DeliveryKind::Event {
        subscription, data, ..
    } = &delivery.kind
    else {
        panic!("an app event");
    };
    assert_eq!(subscription, &one.id);
    assert!(data.is_object());
    // The same delivery again reads the same, so the receipt makes it a duplicate.
    assert_eq!(
        provider
            .parse_delivery(&headers, &body, &secret, now)
            .unwrap(),
        read
    );
    // 4. A wrong secret, a changed body, or a timestamp outside ±300 s is refused.
    assert!(
        provider
            .parse_delivery(&headers, &body, "whsec_some_other_secret", now)
            .is_err()
    );
    let mut changed = body.clone();
    changed.push(b' ');
    assert!(
        provider
            .parse_delivery(&headers, &changed, &secret, now)
            .is_err()
    );
    let (late, body_late, _) = fixture.delivery(&binding, &one.id, "msg_conformance_2", now - 301);
    assert!(
        provider
            .parse_delivery(&late, &body_late, &secret, now)
            .is_err()
    );
    let (early, body_early, _) =
        fixture.delivery(&binding, &one.id, "msg_conformance_3", now + 301);
    assert!(
        provider
            .parse_delivery(&early, &body_early, &secret, now)
            .is_err()
    );
    // 5. Unsubscribe removes it; removing it again still succeeds.
    provider.unsubscribe(&saved, &one.id).await.unwrap();
    assert!(!fixture.subscribed(&one.id));
    provider.unsubscribe(&saved, &one.id).await.unwrap();
    // 6. A broken account is not subscribed.
    fixture.expire(&binding);
    assert!(
        provider
            .subscribe(&saved, &trigger, &json!({}), "https://x.test/")
            .await
            .unwrap_err()
            .downcast_ref::<Refusal>()
            .is_some_and(|r| matches!(r, Refusal::NeedsReconnect))
    );
}

struct Composio(FakeComposio);
impl TriggerFixture for Composio {
    fn provider(&self) -> Arc<dyn ConnectorProvider> {
        composio_for_tests(&self.0)
    }
    fn app(&self) -> &'static str {
        "gmail"
    }
    async fn connected(&self) -> Value {
        let Connect::SignIn { binding, .. } = self
            .provider()
            .connect(
                WORKSPACE,
                "gmail",
                "http://127.0.0.1:1/api/identity/connectors/return/x",
            )
            .await
            .unwrap()
        else {
            panic!("Composio signs in through its page");
        };
        self.0.sign_in(binding["account"].as_str().unwrap());
        binding
    }
    fn trigger(&self) -> String {
        triggers::events("gmail", "composio")[0].trigger.clone()
    }
    fn subscribed(&self, subscription: &str) -> bool {
        self.0
            .state
            .lock()
            .unwrap()
            .triggers
            .contains_key(subscription)
    }
    fn expire(&self, binding: &Value) {
        self.0
            .set_status(binding["account"].as_str().unwrap(), "EXPIRED");
    }
    fn delivery(
        &self,
        binding: &Value,
        subscription: &str,
        id: &str,
        at: i64,
    ) -> (axum::http::HeaderMap, Vec<u8>, String) {
        let body = fake_composio::message(
            id,
            WORKSPACE,
            binding["account"].as_str().unwrap(),
            subscription,
            json!({"sender":"sharma@example.test","subject":"Balance"}),
        );
        let body = serde_json::to_vec(&body).unwrap();
        let secret = fake_composio::WEBHOOK_SECRET;
        (
            fake_composio::signed(secret, id, at, &body),
            body,
            secret.into(),
        )
    }
}

#[tokio::test]
async fn composio_passes_the_event_suite() {
    let fake = Composio(FakeComposio::start().await);
    trigger_conformance(&fake).await;
    assert_eq!(fake.0.state.lock().unwrap().bad_keys, 0);
}

#[tokio::test]
async fn composio_reads_expiry_and_ignores_other_project_events() {
    let provider = composio_for_tests(&FakeComposio::start().await);
    let secret = fake_composio::WEBHOOK_SECRET;
    let now = crate::identity::user::now();
    let read = |body: Value, id: &str| {
        let body = serde_json::to_vec(&body).unwrap();
        provider.parse_delivery(
            &fake_composio::signed(secret, id, now, &body),
            &body,
            secret,
            now,
        )
    };
    let expired = json!({"id":"evt_1","type":"composio.connected_account.expired",
        "metadata":{"project_id":"pr_1","org_id":"ok_1"},
        "data":{"id":"ca_expired","toolkit":{"slug":"gmail"},"status":"EXPIRED"},"timestamp":"2026-11-08T12:00:00Z"});
    let [delivery] = read(expired, "evt_1").unwrap().try_into().unwrap();
    assert_eq!(delivery.kind, DeliveryKind::AccountExpired);
    assert_eq!(delivery.account.as_deref(), Some("ca_expired"));
    let other = json!({"id":"evt_2","type":"composio.connected_account.created","data":{}});
    let [delivery] = read(other, "evt_2").unwrap().try_into().unwrap();
    assert_eq!(delivery.kind, DeliveryKind::Ignored);
    // A trigger message must carry the id it was signed with.
    let message = fake_composio::message("msg_body", WORKSPACE, "ca_1", "ti_1", json!({}));
    assert!(read(message, "msg_header").is_err());
    // So must the workspace's Composio user.
    assert_eq!(
        composio_triggers::workspace_of(&format!("cuckoo-{WORKSPACE}")).as_deref(),
        Some(WORKSPACE)
    );
    assert!(composio_triggers::workspace_of("cuckoo-not-a-workspace").is_none());
    assert!(composio_triggers::workspace_of(WORKSPACE).is_none());
}

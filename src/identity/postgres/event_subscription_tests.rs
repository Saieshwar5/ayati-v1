//! Triggers from a connected app's event (schema 109) against PostgreSQL and
//! the local Composio stand-in: one shared subscription, removal, disconnect,
//! reconnect and the authority a trigger needs.
use super::super::{tests::*, *};
use crate::identity::connectors::{
    self, Connect, ConnectorProvider,
    fake_composio::{Fake, FakeComposio},
};
use crate::identity::control::connections_model::{SystemApproval, WorkspaceConnection};
use crate::identity::events::{
    model::{Change, FromApp},
    subscriptions::{self, Context},
};
use serde_json::json;
use std::sync::Arc;

type Providers = Box<dyn Fn(&str) -> anyhow::Result<Arc<dyn ConnectorProvider>> + Send + Sync>;

pub(super) struct Setup {
    pub db: PgStore,
    pub owner: User,
    pub w: String,
    pub chat: String,
    pub agent: String,
    pub fake: FakeComposio,
    pub provider: Arc<dyn ConnectorProvider>,
    pub connection: String,
    providers: Providers,
}

pub(super) async fn setup() -> Setup {
    let mut db = fixture().await;
    db.pooled = true;
    let owner = user(&db, "app-events-owner").await;
    let w = workspace(&db, &owner).await;
    let (chat, agent) = super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    db.control_system(
        &owner,
        &w,
        &SystemApproval {
            server: "gmail".into(),
            approved: true,
            revision: 0,
        },
    )
    .await
    .unwrap();
    let fake = FakeComposio::start().await;
    let provider = connectors::composio_for_tests(&fake);
    let shared = provider.clone();
    let s = Setup {
        db,
        owner,
        w,
        chat,
        agent,
        fake,
        provider,
        connection: uuid::Uuid::new_v4().to_string(),
        providers: Box::new(move |_| Ok(shared.clone())),
    };
    s.connect().await;
    s.allow("allow").await;
    s
}

impl Setup {
    pub fn state(&self) -> std::sync::MutexGuard<'_, Fake> {
        self.fake.state.lock().unwrap()
    }
    /// Signs the Gmail account in (again) the way the Connections page does.
    pub async fn connect(&self) -> String {
        let revision = query(
            &self.db.db,
            "SELECT revision FROM relay_control_connections WHERE workspace=$1 AND id=$2",
            vec![self.w.clone().into(), self.connection.clone().into()],
        )
        .await
        .unwrap()
        .map(|r| r.try_get::<i64>("", "revision").unwrap())
        .unwrap_or(0);
        let input = WorkspaceConnection {
            id: self.connection.clone(),
            server: "gmail".into(),
            enabled: true,
            revision,
        };
        self.db
            .connection_preflight(&self.owner, &self.w, &input)
            .await
            .unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let callback = format!("http://127.0.0.1:1/api/identity/connectors/return/{id}");
        let Connect::SignIn {
            binding, expires, ..
        } = self
            .provider
            .connect(&self.w, "gmail", &callback)
            .await
            .unwrap()
        else {
            panic!("Composio signs in through its page");
        };
        let start = super::super::connector_providers::StartSignIn {
            id: &id,
            provider: "composio",
            binding: &binding,
            expires,
        };
        self.db
            .connector_sign_in_start(&self.owner, &self.w, &input, &start)
            .await
            .unwrap();
        let account = binding["account"].as_str().unwrap().to_owned();
        self.fake.sign_in(&account);
        let sign_in = self.db.connector_sign_in(&self.owner, &id).await.unwrap();
        connectors::routes::finish_sign_in(
            &self.db,
            &self.owner,
            &id,
            sign_in,
            self.provider.as_ref(),
        )
        .await
        .unwrap();
        account
    }
    /// The workspace rule for Find emails, and the agent's switch.
    pub async fn allow(&self, mode: &str) {
        super::super::tool_test_support::grant(
            &self.db,
            &self.owner,
            &self.w,
            &self.agent,
            &self.connection,
            "gmail.find_emails",
            mode,
        )
        .await;
    }
    pub fn cx(&self) -> Context<'_> {
        Context {
            store: &self.db,
            origin: "https://cuckoo.example.test",
            sealer: None,
            providers: &*self.providers,
            unavailable: Default::default(),
        }
    }
    /// This agent's Gmail choice, as the trigger list shows it.
    pub async fn choice(&self, user: &User) -> Value {
        let choices = self
            .db
            .app_event_choices(user, &self.w, &self.agent, &|_| None)
            .await
            .unwrap();
        choices
            .into_iter()
            .find(|c| c["connection"] == self.connection.as_str())
            .expect("the Gmail account is a choice")
    }
    pub async fn input(&self, filter: Value) -> FromApp {
        let choice = self.choice(&self.owner).await;
        FromApp {
            id: uuid::Uuid::new_v4().to_string(),
            connection: self.connection.clone(),
            event: "gmail.new_email".into(),
            chat: self.chat.clone(),
            name: "Invoice mail".into(),
            instructions: "Read the new email and say whether it asks for payment.".into(),
            connection_revision: choice["authority"]["connection_revision"]
                .as_i64()
                .unwrap_or(0),
            read_revision: choice["authority"]["read_revision"].as_i64().unwrap_or(0),
            delegation_revision: None,
            filter,
        }
    }
    pub async fn create(&self, input: &FromApp) -> anyhow::Result<Value> {
        self.create_as(&self.owner, input).await
    }
    pub async fn create_as(&self, user: &User, input: &FromApp) -> anyhow::Result<Value> {
        subscriptions::create_from_app(&self.cx(), user, &self.w, &self.agent, input).await
    }
    pub async fn subscriptions(&self) -> Vec<Value> {
        json_rows(&self.db.db, "SELECT to_jsonb(s)-'signing_key' AS data FROM relay_connector_subscriptions s WHERE workspace=$1 ORDER BY created_ms,id", vec![self.w.clone().into()]).await.unwrap()
    }
    pub async fn trigger(&self, id: &str) -> Value {
        query(
            &self.db.db,
            "SELECT to_jsonb(t) AS data FROM relay_event_triggers t WHERE workspace=$1 AND id=$2",
            vec![self.w.clone().into(), id.into()],
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "data")
        .unwrap()
    }
    pub async fn change(&self, id: &str, enabled: bool, remove: bool) -> anyhow::Result<Value> {
        let revision = self.trigger(id).await["revision"].as_i64().unwrap();
        let input = Change {
            request: uuid::Uuid::new_v4().to_string(),
            revision,
            enabled,
            connection_revision: None,
            read_revision: None,
            delegation_revision: None,
            remove,
        };
        self.db
            .change_event_trigger(&self.owner, &self.w, &self.agent, id, &input, &[])
            .await
    }
    pub async fn release(&self) -> usize {
        subscriptions::release(&self.db, Some(&self.w), &*self.providers).await
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn app_triggers_share_one_subscription_and_the_last_removal_unsubscribes() {
    let s = setup().await;
    let one = s.input(json!({})).await;
    let saved = s.create(&one).await.unwrap();
    assert_eq!(saved["source_definition"]["event_type"], "gmail.new_email");
    assert_eq!(saved["state"], "enabled");
    assert!(!saved.to_string().contains("GMAIL_NEW_GMAIL_MESSAGE"));
    let two = s.input(json!({"subject":"Invoice"})).await;
    s.create(&two).await.unwrap();
    let subs = s.subscriptions().await;
    assert_eq!(subs.len(), 1, "two triggers share one subscription");
    assert_eq!(s.state().upserts, 1);
    let instance = subs[0]["provider_id"].as_str().unwrap().to_owned();
    assert_eq!(s.state().triggers[&instance].0, "GMAIL_NEW_GMAIL_MESSAGE");
    // An exact repeat returns the saved trigger without calling Composio.
    assert_eq!(s.create(&one).await.unwrap()["id"], saved["id"]);
    assert_eq!(s.state().upserts, 1);
    // The same id with different content, or a filter on an unknown field, is refused.
    let mut changed = one.clone();
    changed.instructions = "Different instructions".into();
    assert!(s.create(&changed).await.is_err());
    assert!(s.create(&s.input(json!({"nope":"x"})).await).await.is_err());
    // Removing one trigger keeps the subscription for the other.
    assert_eq!(
        s.change(&one.id, false, true).await.unwrap()["state"],
        "removed"
    );
    assert_eq!(s.release().await, 0);
    assert!(s.state().triggers.contains_key(&instance));
    assert_eq!(s.subscriptions().await[0]["state"], "active");
    // Removing the last one stops it at Composio.
    s.change(&two.id, false, true).await.unwrap();
    assert_eq!(s.release().await, 1);
    assert!(!s.state().triggers.contains_key(&instance));
    assert!(s.state().deleted_triggers.contains(&instance));
    let subs = s.subscriptions().await;
    assert_eq!(
        (subs[0]["state"].as_str(), subs[0]["released"].as_bool()),
        (Some("removed"), Some(true))
    );
    // Removal is final and removed triggers leave the list.
    assert!(s.change(&two.id, true, false).await.is_err());
    let listed =
        s.db.event_triggers(&s.owner, &s.w, &s.agent, &[])
            .await
            .unwrap();
    assert_eq!(listed["triggers"], json!([]));
    // A new trigger subscribes again.
    s.create(&s.input(json!({})).await).await.unwrap();
    assert_eq!(s.state().upserts, 2);
    assert_eq!(
        s.subscriptions()
            .await
            .iter()
            .filter(|r| r["state"] == "active")
            .count(),
        1
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn reconnecting_breaks_subscriptions_and_disconnecting_removes_them() {
    let s = setup().await;
    let first = s.input(json!({})).await;
    s.create(&first).await.unwrap();
    let old = s.subscriptions().await[0]["provider_id"]
        .as_str()
        .unwrap()
        .to_owned();
    // A reconnect gets a new Composio account: the old subscription can't deliver.
    s.connect().await;
    let subs = s.subscriptions().await;
    assert_eq!(subs[0]["state"], "broken");
    let trigger = s.trigger(&first.id).await;
    assert_eq!(trigger["state"], "blocked");
    assert!(
        trigger["detail"].as_str().unwrap().contains("reconnected"),
        "{trigger}"
    );
    // Needs fixing names the stopped trigger with that reason.
    let now = s.db.activity_now(&s.owner, &s.w, 0, None).await.unwrap();
    assert!(
        now["fixing"].to_string().contains("reconnected"),
        "{}",
        now["fixing"]
    );
    assert_eq!(s.release().await, 1);
    assert!(s.state().deleted_triggers.contains(&old));
    // The reconnected account's rules started over; with Allow again it subscribes anew.
    s.allow("allow").await;
    s.create(&s.input(json!({})).await).await.unwrap();
    let fresh = s.subscriptions().await;
    let active: Vec<_> = fresh.iter().filter(|r| r["state"] == "active").collect();
    assert_eq!(active.len(), 1);
    let new = active[0]["provider_id"].as_str().unwrap().to_owned();
    assert_ne!(new, old);
    // Disconnecting removes its subscriptions and stops them at Composio.
    let revision = query(
        &s.db.db,
        "SELECT revision FROM relay_control_connections WHERE workspace=$1 AND id=$2",
        vec![s.w.clone().into(), s.connection.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get::<i64>("", "revision")
    .unwrap();
    let off = WorkspaceConnection {
        id: s.connection.clone(),
        server: "gmail".into(),
        enabled: false,
        revision,
    };
    s.db.workspace_connection_bind(&s.owner, &s.w, &off, None)
        .await
        .unwrap();
    assert!(
        s.subscriptions()
            .await
            .iter()
            .all(|r| r["state"] != "active")
    );
    assert_eq!(s.release().await, 1);
    assert!(s.state().deleted_triggers.contains(&new));
    assert!(s.state().triggers.is_empty());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn an_app_trigger_needs_an_allowed_read_and_a_person_who_can_ask_the_agent() {
    let s = setup().await;
    // Ask first is not enough for a standing read.
    s.allow("ask").await;
    assert_eq!(s.choice(&s.owner).await["state"], "needs_allow");
    let stale = s.input(json!({})).await;
    assert!(s.create(&stale).await.is_err());
    // A member who owns the agent but can't post where it would work is refused.
    s.allow("allow").await;
    let member = user(&s.db, "app-events-member").await;
    join(&s.db, &s.owner, &s.w, &member, "Member").await;
    execute(
        &s.db.db,
        "UPDATE relay_shared_agents SET owner_id=$3 WHERE workspace=$1 AND id=$2",
        vec![
            s.w.clone().into(),
            s.agent.clone().into(),
            member.id.clone().into(),
        ],
    )
    .await
    .unwrap();
    assert!(
        s.create_as(&member, &s.input(json!({})).await)
            .await
            .is_err()
    );
    assert_eq!(s.state().upserts, 0);
    // A provider that refuses (an expired account) saves nothing and asks for a reconnect.
    let account = s.subscriptions().await;
    assert!(account.is_empty());
    let binding: Value = query(
        &s.db.db,
        "SELECT binding FROM relay_control_connections WHERE workspace=$1 AND id=$2",
        vec![s.w.clone().into(), s.connection.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "binding")
    .unwrap();
    s.fake
        .set_status(binding["account"].as_str().unwrap(), "EXPIRED");
    assert!(s.create(&s.input(json!({})).await).await.is_err());
    assert!(s.subscriptions().await.is_empty());
    assert_eq!(s.choice(&s.owner).await["state"], "reconnect");
    // The app's events are listed under Cuckoo's names for any member.
    let events = s.db.app_events(&member, &s.w, &s.connection).await.unwrap();
    assert_eq!(events["events"][0]["id"], "gmail.new_email");
    assert!(
        events["events"][0]["fields"]
            .as_array()
            .unwrap()
            .contains(&json!("from"))
    );
    let stranger = user(&s.db, "app-events-stranger").await;
    assert!(
        s.db.app_events(&stranger, &s.w, &s.connection)
            .await
            .is_err()
    );
}

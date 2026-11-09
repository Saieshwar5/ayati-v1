//! Schema 108: routes per workspace, saved connections keeping their provider,
//! and reconnecting the same proven account keeping its rules.
use super::super::{tests::*, *};
use crate::identity::chat::model::Job;
use crate::identity::connectors::{
    self, ConnectorProvider,
    connect::{self, Started},
    fake_composio::{FakeComposio, MAILBOX},
    routing,
};
use crate::identity::control::{
    connections_model::{SystemApproval, WorkspaceConnection},
    model::{ActionRequest, Rule},
};
use sea_orm_migration::MigratorTrait;
use serde_json::json;
use std::sync::Arc;

/// A workspace with Gmail approved, an agent and a Composio stand-in.
pub(in crate::identity::postgres) struct Setup {
    pub db: PgStore,
    pub owner: User,
    pub w: String,
    pub agent: String,
    pub chat: String,
    pub fake: FakeComposio,
    pub provider: Arc<dyn ConnectorProvider>,
    pub connection: String,
}

pub(in crate::identity::postgres) async fn setup() -> Setup {
    let db = fixture().await;
    let owner = user(&db, "routing-owner").await;
    let w = workspace(&db, &owner).await;
    let (chat, agent) = super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    let gmail = SystemApproval {
        server: "gmail".into(),
        approved: true,
        revision: 0,
    };
    db.control_system(&owner, &w, &gmail).await.unwrap();
    let fake = FakeComposio::start().await;
    let provider = connectors::composio_for_tests(&fake);
    Setup {
        db,
        owner,
        w,
        agent,
        chat,
        fake,
        provider,
        connection: uuid::Uuid::new_v4().to_string(),
    }
}

impl Setup {
    /// Connects (or reconnects) Gmail the way Apps does: the sign-in page, the
    /// person signing in at Composio, and the return. Returns Composio's account.
    pub async fn connect(&self) -> String {
        let revision = self
            .row()
            .await
            .map_or(0, |r| r["revision"].as_i64().unwrap());
        let input = WorkspaceConnection {
            id: self.connection.clone(),
            server: "gmail".into(),
            enabled: true,
            revision,
        };
        let origin = "http://127.0.0.1:1";
        let Started::SignIn(url) = connect::start(
            &self.db,
            origin,
            &self.owner,
            &self.w,
            &input,
            self.provider.as_ref(),
        )
        .await
        .unwrap() else {
            panic!("Composio signs in through its page");
        };
        let account = url.rsplit('/').next().unwrap().to_owned();
        self.fake.sign_in(&account);
        let id: String = query(
            &self.db.db,
            "SELECT id FROM relay_connector_sign_ins WHERE workspace=$1 AND binding->>'account'=$2",
            vec![self.w.clone().into(), account.clone().into()],
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "id")
        .unwrap();
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
    pub async fn row(&self) -> Option<Value> {
        query(&self.db.db, "SELECT to_jsonb(c) AS data FROM relay_control_connections c WHERE workspace=$1 AND id=$2", vec![self.w.clone().into(), self.connection.clone().into()])
            .await.unwrap().map(|r| r.try_get("", "data").unwrap())
    }
    pub async fn count(&self, table: &str) -> i64 {
        query(
            &self.db.db,
            &format!("SELECT count(*) AS n FROM {table} WHERE workspace=$1 AND connection=$2"),
            vec![self.w.clone().into(), self.connection.clone().into()],
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "n")
        .unwrap()
    }
    pub async fn grant(&self, operation: &str, mode: &str) {
        let (db, owner) = (&self.db, &self.owner);
        super::super::tool_test_support::grant(
            db,
            owner,
            &self.w,
            &self.agent,
            &self.connection,
            operation,
            mode,
        )
        .await;
    }
    pub async fn job(&self, text: &str) -> Job {
        let sent = message(text, Some(&self.agent));
        self.db
            .chat_send(&self.owner, &self.w, &self.chat, &sent)
            .await
            .unwrap();
        self.db.claim_job().await.unwrap().unwrap()
    }
    /// Asks for one action and sends it through the gateway when it may run.
    pub async fn request(&self, job: &Job, operation: &str, arguments: Value) -> Value {
        let request = ActionRequest {
            connection: self.connection.clone(),
            operation: operation.into(),
            arguments,
        };
        let proposed = self.db.control_propose(job, &request).await.unwrap();
        match proposed.dispatch {
            None => proposed.action,
            Some(dispatch) => crate::identity::control::dispatch_with(
                &self.db,
                &self.w,
                dispatch,
                self.provider.as_ref(),
            )
            .await
            .unwrap(),
        }
    }
    async fn action_state(&self, id: &str) -> String {
        query(
            &self.db.db,
            "SELECT state FROM relay_control_actions WHERE id=$1",
            vec![id.into()],
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "state")
        .unwrap()
    }
}

/// The rules and switches that "keep" must keep.
async fn kept(s: &Setup) -> [i64; 4] {
    [
        s.count("relay_agent_connections").await,
        s.count("relay_connector_rules").await,
        s.count("relay_control_rules").await,
        s.count("relay_action_meanings").await,
    ]
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn workspace_routes_choose_the_provider_for_new_connections_while_saved_ones_keep_theirs() {
    let s = setup().await;
    s.connect().await;
    s.grant("gmail.find_emails", "allow").await;
    s.db.connector_route_set(
        &s.owner,
        &s.w,
        "gmail",
        Some("pipedream"),
        "Trying Pipedream",
    )
    .await
    .unwrap();
    let routes = connectors::routes_for(&s.db, &s.w).await.unwrap();
    assert_eq!(routes.workspace["gmail"], "pipedream");
    let other = workspace(&s.db, &s.owner).await;
    assert!(
        connectors::routes_for(&s.db, &other)
            .await
            .unwrap()
            .workspace
            .is_empty()
    );
    // Where two providers map the app, the workspace's route chooses.
    let both: connectors::curated::CatalogApp = serde_json::from_value(json!({"id":"gmail","name":"Gmail",
        "category":"email_calendar","description":"Email","providers":{"composio":"gmail","pipedream":"gmail"},"actions":[]}))
    .unwrap();
    assert_eq!(
        routing::resolve(&both, &routes, &["mcp", "composio", "pipedream"]),
        Some("pipedream")
    );
    // The shipped catalog maps Gmail to Composio alone, so Composio still serves it.
    let offered = routing::apps_with(std::slice::from_ref(&s.provider), &s.w, &routes).unwrap();
    assert_eq!(
        (offered[0].id.as_str(), offered[0].provider),
        ("gmail", "composio")
    );
    // The saved connection keeps the provider it was made with, and keeps working.
    assert_eq!(s.row().await.unwrap()["provider"], "composio");
    let job = s.job("Any mail from Sharma?").await;
    let done = s
        .request(&job, "gmail.find_emails", json!({"query":"from:sharma"}))
        .await;
    assert_eq!(done["state"], "completed", "{done}");
    // Clearing the route returns the app to the defaults; bad input is refused.
    s.db.connector_route_set(&s.owner, &s.w, "gmail", None, "")
        .await
        .unwrap();
    assert!(
        connectors::routes_for(&s.db, &s.w)
            .await
            .unwrap()
            .workspace
            .is_empty()
    );
    let missing = uuid::Uuid::new_v4().to_string();
    assert!(
        s.db.connector_route_set(&s.owner, &missing, "gmail", Some("composio"), "")
            .await
            .is_err()
    );
    assert!(
        s.db.connector_route_set(&s.owner, &s.w, "gmail", Some("composio"), "two\nlines")
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn reconnecting_the_same_account_keeps_its_rules_and_cancels_waiting_approvals() {
    let s = setup().await;
    let first = s.connect().await;
    s.grant("gmail.find_emails", "allow").await;
    s.grant("gmail.send_email", "ask").await;
    let agent_rule = Rule {
        connection: s.connection.clone(),
        operation: "gmail.draft_email".into(),
        mode: "block".into(),
        revision: 0,
    };
    s.db.control_rule(&s.owner, &s.w, &s.agent, &agent_rule)
        .await
        .unwrap();
    let meaning = super::super::control_actions::guard::ActionMeaning {
        connection: s.connection.clone(),
        operation: "gmail.draft_email".into(),
        meaning: "change".into(),
        revision: 0,
    };
    s.db.action_meaning_save(&s.owner, &s.w, &meaning)
        .await
        .unwrap();
    let job = s.job("Send Sharma the balance").await;
    let send = json!({"to":"sharma@example.test","subject":"Balance","body":"Due"});
    let waiting = s.request(&job, "gmail.send_email", send.clone()).await;
    assert_eq!(waiting["state"], "pending", "{waiting}");
    let before = s.row().await.unwrap();
    assert_eq!(before["definition"]["account"], MAILBOX);
    let rules = kept(&s).await;
    assert_eq!(rules, [1, 2, 1, 1]);
    // The account's sign-in expired; the admin reconnects the same mailbox.
    s.db.connector_needs_reconnect(&s.w, &s.connection)
        .await
        .unwrap();
    let second = s.connect().await;
    let after = s.row().await.unwrap();
    assert_ne!(first, second);
    assert_eq!(after["binding"]["account"].as_str(), Some(second.as_str()));
    assert_eq!(after["identity"], before["identity"]);
    assert!(after["identity"].is_string());
    assert!(after["revision"].as_i64() > before["revision"].as_i64());
    assert_eq!(kept(&s).await, rules, "switches, rules and meanings stay");
    assert_eq!(
        s.action_state(waiting["id"].as_str().unwrap()).await,
        "invalidated"
    );
    assert!(
        s.fake.state.lock().unwrap().removed.contains(&first),
        "the old account is revoked"
    );
    // The agent goes on with the same rules: searching runs, sending asks again.
    let job = s.job("Any mail from Sharma?").await;
    let found = s
        .request(&job, "gmail.find_emails", json!({"query":"from:sharma"}))
        .await;
    assert_eq!(found["state"], "completed", "{found}");
    assert_eq!(
        s.request(&job, "gmail.send_email", send).await["state"],
        "pending"
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn another_or_unproven_account_or_a_changed_review_starts_over() {
    let s = setup().await;
    s.connect().await;
    s.grant("gmail.find_emails", "allow").await;
    // Another mailbox signed in.
    s.fake.state.lock().unwrap().mailbox = Some("billing@example.test".into());
    s.connect().await;
    assert_eq!(kept(&s).await, [0, 0, 0, 0]);
    assert_eq!(
        s.row().await.unwrap()["definition"]["account"],
        "billing@example.test"
    );
    // The provider couldn't prove the mailbox: no identity, so it starts over.
    s.grant("gmail.find_emails", "allow").await;
    s.fake.state.lock().unwrap().mailbox = None;
    s.connect().await;
    assert_eq!(kept(&s).await, [0, 0, 0, 0]);
    assert!(s.row().await.unwrap()["identity"].is_null());
    // Proven again after an unproven one: still a different account record.
    s.grant("gmail.find_emails", "allow").await;
    s.fake.state.lock().unwrap().mailbox = Some(MAILBOX.into());
    s.connect().await;
    assert_eq!(kept(&s).await, [0, 0, 0, 0]);
    // The same mailbox, but Composio's actions changed since the review.
    s.grant("gmail.find_emails", "allow").await;
    s.fake.state.lock().unwrap().changed = true;
    s.connect().await;
    assert_eq!(kept(&s).await, [0, 0, 0, 0]);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn the_upgrade_keeps_connections_and_records_who_connected_them_without_fencing_work() {
    let url = std::env::var("RELAY_TEST_DATABASE_URL").unwrap();
    let admin = PgStore::connect(&url).await.unwrap();
    let schema = format!("relay_routing_upgrade_{}", uuid::Uuid::new_v4().simple());
    admin
        .db
        .execute_unprepared(&format!("CREATE SCHEMA {schema}"))
        .await
        .unwrap();
    let mut url = reqwest::Url::parse(&url).unwrap();
    url.query_pairs_mut()
        .append_pair("options", &format!("-c search_path={schema}"));
    let db = PgStore::connect(url.as_str()).await.unwrap();
    let before = migration::Migrator::migrations()
        .iter()
        .position(|m| m.name() == "m20261108_000108_connector_routing")
        .unwrap();
    migration::Migrator::up(&db.db, Some(before as u32))
        .await
        .unwrap();
    let owner = user(&db, "upgrade-owner").await;
    let w = workspace(&db, &owner).await;
    let (_, agent) = super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    execute(&db.db, "INSERT INTO relay_control_systems(workspace,server,approved,revision) VALUES($1,'gmail',true,1)", vec![w.clone().into()]).await.unwrap();
    execute(&db.db, r#"INSERT INTO relay_control_connections(workspace,id,server,definition,fingerprint,provider,binding) VALUES($1,'account','gmail','{"id":"gmail","name":"Gmail","account":null,"operations":[]}','reviewed','composio','{"account":"ca_1"}')"#, vec![w.clone().into()]).await.unwrap();
    execute(&db.db, "INSERT INTO relay_audit(actor,workspace,action,target,at) VALUES($1,$2,'workspace_connection_saved','account',1234)", vec![owner.id.clone().into(), w.clone().into()]).await.unwrap();
    execute(&db.db, "INSERT INTO relay_agent_connections(workspace,agent,connection,revision,enabled_by,at) VALUES($1,$2,'account',nextval('relay_control_revision'),$3,1)", vec![w.clone().into(), agent.clone().into(), owner.id.clone().into()]).await.unwrap();
    let epochs = "SELECT COALESCE(sum(revision),0)::bigint AS n FROM relay_context_epochs WHERE workspace=$1";
    let fenced = |row: Option<sea_orm::QueryResult>| row.unwrap().try_get::<i64>("", "n").unwrap();
    let epochs_before = fenced(query(&db.db, epochs, vec![w.clone().into()]).await.unwrap());
    db.migrate().await.unwrap();
    db.verify_schema().await.unwrap();
    let row: Value = query(
        &db.db,
        "SELECT to_jsonb(c) AS data FROM relay_control_connections c WHERE workspace=$1",
        vec![w.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "data")
    .unwrap();
    assert_eq!(
        (row["connected_by"].as_str(), row["connected_at"].as_i64()),
        (Some(owner.id.as_str()), Some(1234))
    );
    assert!(row["identity"].is_null());
    assert_eq!(
        (row["revision"].as_i64(), row["provider"].as_str()),
        (Some(1), Some("composio"))
    );
    let epochs_after = fenced(query(&db.db, epochs, vec![w.clone().into()]).await.unwrap());
    assert_eq!(
        epochs_after, epochs_before,
        "recording history fences nothing"
    );
    let apps = db.workspace_apps(&owner, &w).await.unwrap();
    assert_eq!(apps["apps"][0]["connected"]["at"], 1234);
    assert_eq!(apps["apps"][0]["state"], "ok");
    assert_eq!(apps["apps"][0]["agents"].as_array().unwrap().len(), 1);
}

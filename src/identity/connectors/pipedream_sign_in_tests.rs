//! A Pipedream sign-in through Cuckoo's own sign-in records and connection
//! binding (PostgreSQL): saving the account, and reconnecting it.
use super::fake_pipedream::FakePipedream;
use super::*;
use crate::identity::User;
use crate::identity::control::connections_model::{SystemApproval, WorkspaceConnection};
use crate::identity::postgres::{
    PgStore,
    connector_providers::StartSignIn,
    tests::{fixture, user, workspace},
};

struct Setup {
    db: PgStore,
    owner: User,
    w: String,
    fake: FakePipedream,
    provider: Arc<dyn ConnectorProvider>,
    connection: String,
}

impl Setup {
    async fn new() -> Self {
        let db = fixture().await;
        let owner = user(&db, "pipedream-owner").await;
        let w = workspace(&db, &owner).await;
        let approval = SystemApproval {
            server: "gmail".into(),
            approved: true,
            revision: 0,
        };
        db.control_system(&owner, &w, &approval).await.unwrap();
        let fake = FakePipedream::start().await;
        let provider = pipedream_for_tests(&fake);
        Self {
            db,
            owner,
            w,
            fake,
            provider,
            connection: uuid::Uuid::new_v4().to_string(),
        }
    }

    /// The connection as Settings lists it.
    async fn listed(&self) -> Value {
        let data = self
            .db
            .workspace_connections(&self.owner, &self.w)
            .await
            .unwrap();
        data["connections"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == self.connection.as_str())
            .cloned()
            .unwrap_or(Value::Null)
    }

    /// Signs in the way the Connections endpoint and return route do; `again`
    /// signs in to the account Pipedream already holds, updated in place.
    async fn sign_in(&self, again: bool) -> anyhow::Result<()> {
        let revision = self.listed().await["revision"].as_i64().unwrap_or(0);
        let input = WorkspaceConnection {
            id: self.connection.clone(),
            server: "gmail".into(),
            enabled: true,
            revision,
        };
        self.db
            .connection_preflight(&self.owner, &self.w, &input)
            .await?;
        let id = uuid::Uuid::new_v4().to_string();
        let callback = format!("http://127.0.0.1:1/api/identity/connectors/return/{id}");
        let Connect::SignIn {
            binding, expires, ..
        } = self.provider.connect(&self.w, "gmail", &callback).await?
        else {
            panic!("Pipedream signs in through Connect Link");
        };
        let start = StartSignIn {
            id: &id,
            provider: "pipedream",
            binding: &binding,
            expires,
        };
        self.db
            .connector_sign_in_start(&self.owner, &self.w, &input, &start)
            .await?;
        if again {
            self.fake.sign_in_again(&binding);
        } else {
            self.fake.sign_in(&binding);
        }
        let sign_in = self.db.connector_sign_in(&self.owner, &id).await?;
        routes::finish_sign_in(&self.db, &self.owner, &id, sign_in, self.provider.as_ref()).await
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_pipedream_account_is_saved_and_reconnecting_revokes_only_a_replaced_account() {
    let s = Setup::new().await;
    s.sign_in(false).await.unwrap();
    let listed = s.listed().await;
    assert_eq!(listed["provider"], "pipedream");
    assert_eq!(
        listed["definition"]["account"],
        super::fake_pipedream::MAILBOX
    );
    assert!(!listed.to_string().contains(super::fake_pipedream::TOKEN));
    // Signing in again to the same upstream account, which Pipedream updated in
    // place, keeps the account: nothing is revoked.
    s.sign_in(true).await.unwrap();
    assert!(s.fake.state.lock().unwrap().removed.is_empty());
    assert_eq!(s.fake.state.lock().unwrap().accounts.len(), 1);
    // A new account replaces the old one, which Pipedream removes.
    s.sign_in(false).await.unwrap();
    let state = s.fake.state.lock().unwrap();
    assert_eq!(state.removed, ["apn_1"]);
    assert_eq!(state.accounts.len(), 1);
    assert_eq!(state.accounts[0].id, "apn_2");
}

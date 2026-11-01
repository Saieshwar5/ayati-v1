use super::super::{tests::*, *};
use super::StartSignIn;
use crate::identity::connectors::{
    self, Connect, ConnectorProvider,
    fake_composio::{FakeComposio, TOKEN},
};
use crate::identity::control::{
    connections_model::{SystemApproval, WorkspaceConnection},
    model::ActionRequest,
};
use serde_json::json;
use std::sync::Arc;

struct Setup {
    db: PgStore,
    owner: User,
    w: String,
    agent: String,
    chat: String,
    fake: FakeComposio,
    provider: Arc<dyn ConnectorProvider>,
    connection: String,
}

async fn setup() -> Setup {
    let db = fixture().await;
    let owner = user(&db, "connector-owner").await;
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
    fn input(&self, revision: i64) -> WorkspaceConnection {
        WorkspaceConnection {
            id: self.connection.clone(),
            server: "gmail".into(),
            enabled: true,
            revision,
        }
    }
    /// Starts a sign-in the way the Connections endpoint does; returns its id
    /// and Composio's account id.
    async fn start(&self, revision: i64) -> (String, String) {
        let input = self.input(revision);
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
        let start = StartSignIn {
            id: &id,
            provider: "composio",
            binding: &binding,
            expires,
        };
        self.db
            .connector_sign_in_start(&self.owner, &self.w, &input, &start)
            .await
            .unwrap();
        (id, binding["account"].as_str().unwrap().to_owned())
    }
    async fn finish(&self, id: &str) -> anyhow::Result<()> {
        let sign_in = self.db.connector_sign_in(&self.owner, id).await?;
        connectors::routes::finish_sign_in(
            &self.db,
            &self.owner,
            id,
            sign_in,
            self.provider.as_ref(),
        )
        .await
    }
    async fn connection(&self) -> Value {
        query(&self.db.db, "SELECT to_jsonb(c) AS data FROM relay_control_connections c WHERE workspace=$1 AND id=$2", vec![self.w.clone().into(), self.connection.clone().into()])
            .await.unwrap().unwrap().try_get("", "data").unwrap()
    }
    async fn needs_reconnect(&self) -> bool {
        let data = self
            .db
            .workspace_connections(&self.owner, &self.w)
            .await
            .unwrap();
        data["connections"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["id"] == self.connection.as_str() && c["needs_reconnect"] == true)
    }
    async fn job(&self, text: &str) -> crate::identity::chat::model::Job {
        self.db
            .chat_send(
                &self.owner,
                &self.w,
                &self.chat,
                &message(text, Some(&self.agent)),
            )
            .await
            .unwrap();
        self.db.claim_job().await.unwrap().unwrap()
    }
    /// Asks for one Gmail search and sends it through the gateway.
    async fn search(&self, job: &crate::identity::chat::model::Job, query: &str) -> Value {
        let request = ActionRequest {
            connection: self.connection.clone(),
            operation: "gmail.find_emails".into(),
            arguments: json!({"query":query}),
        };
        let proposed = self.db.control_propose(job, &request).await.unwrap();
        let Some(dispatch) = proposed.dispatch else {
            return proposed.action;
        };
        crate::identity::control::dispatch_with(&self.db, &self.w, dispatch, self.provider.as_ref())
            .await
            .unwrap()
    }
    fn executed(&self) -> usize {
        self.fake.state.lock().unwrap().executed.len()
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_composio_account_signs_in_runs_through_the_gateway_and_stops_when_its_provider_is_paused()
 {
    let s = setup().await;
    let (sign_in, account) = s.start(0).await;
    // Only the admin who started the sign-in can finish it.
    let member = user(&s.db, "connector-member").await;
    join(&s.db, &s.owner, &s.w, &member, "Member").await;
    assert!(s.db.connector_sign_in(&member, &sign_in).await.is_err());
    s.fake.sign_in(&account);
    s.finish(&sign_in).await.unwrap();
    assert!(
        s.db.connector_sign_in(&s.owner, &sign_in).await.is_err(),
        "a sign-in is used once"
    );
    let saved = s.connection().await;
    assert_eq!(
        (
            saved["provider"].as_str(),
            saved["binding"]["account"].as_str()
        ),
        (Some("composio"), Some(account.as_str()))
    );
    assert!(
        !saved.to_string().contains(TOKEN),
        "the provider's token is never stored"
    );
    // The agent uses it under Cuckoo's names; Composio gets its own.
    super::super::tool_test_support::grant(
        &s.db,
        &s.owner,
        &s.w,
        &s.agent,
        &s.connection,
        "gmail.find_emails",
        "allow",
    )
    .await;
    let job = s.job("Any mail from Sharma?").await;
    let listed = s.db.control_operations(&job).await.unwrap().to_string();
    assert!(
        listed.contains("gmail.find_emails") && !listed.contains("GMAIL_FETCH_EMAILS"),
        "{listed}"
    );
    let done = s.search(&job, "from:sharma").await;
    assert_eq!(done["state"], "completed", "{done}");
    assert_eq!(
        s.fake.state.lock().unwrap().executed[0],
        ("GMAIL_FETCH_EMAILS".into(), json!({"query":"from:sharma"}))
    );
    // Cuckoo switches Composio off: its actions disappear and nothing is sent.
    s.db.connector_provider_set(&s.owner, "composio", false, "Provider incident")
        .await
        .unwrap();
    assert!(
        !s.db
            .control_operations(&job)
            .await
            .unwrap()
            .to_string()
            .contains("gmail.find_emails")
    );
    let held = s.search(&job, "from:ravi").await;
    assert_eq!(held["state"], "invalidated");
    assert!(
        held["result"]
            .as_str()
            .unwrap()
            .contains("Provider incident"),
        "{held}"
    );
    assert_eq!(s.executed(), 1);
    s.db.connector_provider_set(&s.owner, "composio", true, "")
        .await
        .unwrap();
    // The same request in the same task returns its first receipt; a new one runs.
    let resumed = s.search(&job, "from:meera").await;
    assert_eq!(resumed["state"], "completed", "{resumed}");
    // Switching off needs a reason people can read.
    assert!(
        s.db.connector_provider_set(&s.owner, "composio", false, " ")
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn an_expired_account_asks_for_a_reconnect_and_reconnecting_replaces_and_revokes_the_old_one()
{
    let s = setup().await;
    let (sign_in, account) = s.start(0).await;
    s.fake.sign_in(&account);
    s.finish(&sign_in).await.unwrap();
    super::super::tool_test_support::grant(
        &s.db,
        &s.owner,
        &s.w,
        &s.agent,
        &s.connection,
        "gmail.find_emails",
        "allow",
    )
    .await;
    let job = s.job("Any mail from Sharma?").await;
    // Google expired the grant: nothing is sent and the account is marked.
    s.fake.set_status(&account, "EXPIRED");
    let refused = s.search(&job, "from:sharma").await;
    assert_eq!(refused["state"], "invalidated");
    assert!(
        refused["result"]
            .as_str()
            .unwrap()
            .contains("needs to be reconnected"),
        "{refused}"
    );
    assert!(s.needs_reconnect().await);
    // The agent's work goes on: it can tell the person the account needs reconnecting.
    let reply = query(
        &s.db.db,
        "SELECT state FROM relay_chat_messages WHERE id=$1",
        vec![job.reply.clone().into()],
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(reply.try_get::<String>("", "state").unwrap(), "queued");
    s.db.control_operations(&job)
        .await
        .expect("the running task keeps its authority");
    assert!(
        !s.db
            .control_operations(&job)
            .await
            .unwrap()
            .to_string()
            .contains("gmail.find_emails")
    );
    assert_eq!(s.executed(), 0);
    // An admin reconnects: a new account replaces the old one, which is revoked.
    let revision = s.connection().await["revision"].as_i64().unwrap();
    let (again, fresh) = s.start(revision).await;
    s.fake.sign_in(&fresh);
    s.finish(&again).await.unwrap();
    let saved = s.connection().await;
    assert!(!s.needs_reconnect().await);
    assert_eq!(saved["binding"]["account"].as_str(), Some(fresh.as_str()));
    assert!(s.fake.state.lock().unwrap().removed.contains(&account));
    // As with any reconnect, the account's actions start over from the safe defaults.
    let switches = query(
        &s.db.db,
        "SELECT count(*) AS n FROM relay_agent_connections WHERE workspace=$1 AND connection=$2",
        vec![s.w.clone().into(), s.connection.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get::<i64>("", "n")
    .unwrap();
    assert_eq!(switches, 0);
    // Disconnecting hands the account back for the provider to revoke.
    let input = WorkspaceConnection {
        enabled: false,
        ..s.input(saved["revision"].as_i64().unwrap())
    };
    let released =
        s.db.workspace_connection_bind(&s.owner, &s.w, &input, None)
            .await
            .unwrap()
            .unwrap();
    assert_eq!(
        (
            released.provider.as_str(),
            released.binding["account"].as_str()
        ),
        ("composio", Some(fresh.as_str()))
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_sign_in_that_fails_or_expires_saves_nothing_and_cleans_up_at_the_provider() {
    let s = setup().await;
    let (sign_in, account) = s.start(0).await;
    // The person gave up on the provider's page.
    s.fake.set_status(&account, "FAILED");
    assert!(s.finish(&sign_in).await.is_err());
    assert!(s.fake.state.lock().unwrap().removed.contains(&account));
    let state = query(
        &s.db.db,
        "SELECT state FROM relay_connector_sign_ins WHERE id=$1",
        vec![sign_in.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get::<String>("", "state")
    .unwrap();
    assert_eq!(state, "failed");
    assert!(
        query(
            &s.db.db,
            "SELECT id FROM relay_control_connections WHERE workspace=$1",
            vec![s.w.clone().into()]
        )
        .await
        .unwrap()
        .is_none()
    );
    // A sign-in left open too long can't be finished.
    let (late, _) = s.start(0).await;
    execute(
        &s.db.db,
        "UPDATE relay_connector_sign_ins SET expires=0 WHERE id=$1",
        vec![late.clone().into()],
    )
    .await
    .unwrap();
    let Err(expired) = s.db.connector_sign_in(&s.owner, &late).await else {
        panic!("an expired sign-in finished")
    };
    assert!(expired.to_string().contains("expired"));
    // Removing the app's approval disconnects it and hands the account back for revoking.
    let (ok, fresh) = s.start(0).await;
    s.fake.sign_in(&fresh);
    s.finish(&ok).await.unwrap();
    let released =
        s.db.control_system(
            &s.owner,
            &s.w,
            &SystemApproval {
                server: "gmail".into(),
                approved: false,
                revision: 1,
            },
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(released.binding["account"].as_str(), Some(fresh.as_str()));
}

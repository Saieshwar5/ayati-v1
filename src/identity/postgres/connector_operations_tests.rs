//! Provider operations against real PostgreSQL and a local Composio stand-in:
//! call metrics, the circuit breaker shared by controllers, the rate limit and
//! `last_used` on the Apps page. The drift check is in `connector_drift_tests.rs`.
use super::super::connector_providers::StartSignIn;
use super::super::tests::{fixture, message, user, workspace};
use super::super::*;
use super::{BREAKER_FAILURES, BREAKER_REASON, RATE_PER_MINUTE, RATE_REASON};
use crate::identity::chat::model::Job;
use crate::identity::connectors::{self, Connect, ConnectorProvider, fake_composio::FakeComposio};
use crate::identity::control::{
    connections_model::{SystemApproval, WorkspaceConnection},
    model::ActionRequest,
};
use serde_json::json;
use std::sync::Arc;

pub(super) struct Setup {
    pub db: PgStore,
    pub owner: User,
    pub w: String,
    pub agent: String,
    pub chat: String,
    pub fake: FakeComposio,
    pub provider: Arc<dyn ConnectorProvider>,
    pub connection: String,
}

/// A workspace with Gmail connected through Composio, which its agent may search.
pub(super) async fn setup() -> Setup {
    let db = fixture().await;
    let owner = user(&db, "operations-owner").await;
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
    let connection = uuid::Uuid::new_v4().to_string();
    let input = WorkspaceConnection {
        id: connection.clone(),
        server: "gmail".into(),
        enabled: true,
        revision: 0,
    };
    db.connection_preflight(&owner, &w, &input).await.unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let callback = format!("http://127.0.0.1:1/api/identity/connectors/return/{id}");
    let Connect::SignIn {
        binding, expires, ..
    } = provider.connect(&w, "gmail", &callback).await.unwrap()
    else {
        panic!("Composio signs in through its page");
    };
    let start = StartSignIn {
        id: &id,
        provider: "composio",
        binding: &binding,
        expires,
    };
    db.connector_sign_in_start(&owner, &w, &input, &start)
        .await
        .unwrap();
    fake.sign_in(binding["account"].as_str().unwrap());
    let sign_in = db.connector_sign_in(&owner, &id).await.unwrap();
    connectors::routes::finish_sign_in(&db, &owner, &id, sign_in, provider.as_ref())
        .await
        .unwrap();
    super::super::tool_test_support::grant(
        &db,
        &owner,
        &w,
        &agent,
        &connection,
        "gmail.find_emails",
        "allow",
    )
    .await;
    Setup {
        db,
        owner,
        w,
        agent,
        chat,
        fake,
        provider,
        connection,
    }
}

impl Setup {
    async fn job(&self) -> Job {
        self.db
            .chat_send(
                &self.owner,
                &self.w,
                &self.chat,
                &message("Any mail?", Some(&self.agent)),
            )
            .await
            .unwrap();
        self.db.claim_job().await.unwrap().unwrap()
    }
    /// One Gmail search through the gateway, on `store`.
    async fn search(&self, store: &PgStore, job: &Job, query: &str) -> Value {
        let request = ActionRequest {
            connection: self.connection.clone(),
            operation: "gmail.find_emails".into(),
            arguments: json!({ "query": query }),
        };
        let proposed = store.control_propose(job, &request).await.unwrap();
        let dispatch = proposed.dispatch.expect("allowed actions run at once");
        crate::identity::control::dispatch_with(store, &self.w, dispatch, self.provider.as_ref())
            .await
            .unwrap()
    }
    fn requests(&self) -> usize {
        self.fake.state.lock().unwrap().requests
    }
    async fn calls(&self) -> Vec<Value> {
        json_rows(&self.db.db, "SELECT to_jsonb(k) AS data FROM relay_connector_calls k WHERE workspace=$1 ORDER BY started_ms,id", vec![self.w.clone().into()])
            .await
            .unwrap()
    }
    async fn breaker(&self) -> Value {
        let breakers = self.db.connector_breakers().await.unwrap();
        breakers
            .into_iter()
            .find(|b| b["provider"] == "composio")
            .unwrap_or(json!({"state":"closed"}))
    }
    pub(super) async fn sql(&self, sql: &str) {
        execute(&self.db.db, sql, vec![]).await.unwrap();
    }
}

/// A second controller's store on the same database.
pub(super) async fn second_store(db: &PgStore) -> PgStore {
    let schema: String = query(&db.db, "SELECT current_schema() AS s", vec![])
        .await
        .unwrap()
        .unwrap()
        .try_get("", "s")
        .unwrap();
    let mut url = reqwest::Url::parse(&std::env::var("RELAY_TEST_DATABASE_URL").unwrap()).unwrap();
    url.query_pairs_mut()
        .append_pair("options", &format!("-c search_path={schema}"));
    url.set_username("relay_app").unwrap();
    PgStore::connect(url.as_str()).await.unwrap()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn every_call_is_recorded_with_its_outcome_and_the_apps_page_shows_when_it_was_last_used() {
    let s = setup().await;
    let job = s.job().await;
    let apps = s.db.workspace_apps(&s.owner, &s.w).await.unwrap();
    assert_eq!(apps["apps"][0]["last_used"], Value::Null, "never used yet");
    assert_eq!(
        s.search(&s.db, &job, "from:sharma").await["state"],
        "completed"
    );
    s.fake.state.lock().unwrap().fail_execute = true;
    assert_eq!(
        s.search(&s.db, &job, "from:ravi").await["state"],
        "uncertain"
    );
    s.fake.state.lock().unwrap().fail_execute = false;
    // Composio refuses Cuckoo's key: a provider error while checking, nothing sent.
    let key = std::fs::read_to_string(&s.fake.key_file).unwrap();
    std::fs::write(&s.fake.key_file, "ak_rotated_elsewhere").unwrap();
    assert_eq!(
        s.search(&s.db, &job, "from:meera").await["state"],
        "invalidated"
    );
    std::fs::write(&s.fake.key_file, key).unwrap();
    // Cuckoo's switch refuses before admission.
    s.db.connector_provider_set(&s.owner, "composio", false, "Provider incident")
        .await
        .unwrap();
    assert_eq!(
        s.search(&s.db, &job, "from:asha").await["state"],
        "invalidated"
    );
    s.db.connector_provider_set(&s.owner, "composio", true, "")
        .await
        .unwrap();
    // The account expired: the provider explained, which is not a provider error.
    let account = s
        .fake
        .state
        .lock()
        .unwrap()
        .accounts
        .keys()
        .next()
        .unwrap()
        .clone();
    s.fake.set_status(&account, "EXPIRED");
    assert_eq!(
        s.search(&s.db, &job, "from:kiran").await["state"],
        "invalidated"
    );

    let calls = s.calls().await;
    let seen: Vec<(&str, bool, Option<&str>)> = calls
        .iter()
        .map(|c| {
            (
                c["outcome"].as_str().unwrap(),
                c["provider_error"] == true,
                c["refused"].as_str(),
            )
        })
        .collect();
    assert_eq!(
        seen,
        [
            ("completed", false, None),
            ("uncertain", false, None),
            ("preflight_failed", true, None),
            ("refused", false, Some("paused")),
            ("preflight_failed", false, None),
        ]
    );
    for call in &calls {
        assert_eq!(
            (
                call["provider"].as_str(),
                call["app"].as_str(),
                call["agent"].as_str()
            ),
            (Some("composio"), Some("gmail"), Some(s.agent.as_str()))
        );
        assert!(
            call["latency_ms"].as_i64().is_some() && call["usd_estimate"] == 0.0,
            "{call}"
        );
        assert!(
            !call.to_string().contains("from:"),
            "arguments are never recorded"
        );
    }
    // The Apps page: the uncertain search was the last that reached Gmail.
    let apps = s.db.workspace_apps(&s.owner, &s.w).await.unwrap();
    let name: String = query(
        &s.db.db,
        "SELECT name FROM relay_shared_agents WHERE id=$1",
        vec![s.agent.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "name")
    .unwrap();
    assert_eq!(
        apps["apps"][0]["last_used"],
        json!({"at": calls[1]["started_ms"], "agent_name": name})
    );
    // Control: this month's calls, errors and refusals for Composio.
    let health = s.db.connector_health(&["mcp", "composio"]).await.unwrap();
    let composio = health["health"]
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["provider"] == "composio")
        .unwrap();
    let month = &composio["month"];
    assert_eq!(
        (
            month["calls"].as_i64(),
            month["errors"].as_i64(),
            month["refused"].as_i64()
        ),
        (Some(4), Some(2), Some(1))
    );
    assert_eq!(
        month["usd"].as_f64(),
        Some(0.0),
        "priced from the providers file once it is wired in"
    );
    assert_eq!(composio["breaker"]["state"], "closed");
    assert_eq!(health["health"][0]["provider"], "mcp");
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn the_breaker_opens_after_repeated_failures_for_every_controller_and_lets_one_trial_through()
{
    let s = setup().await;
    let other = second_store(&s.db).await;
    let job = s.job().await;
    s.fake.state.lock().unwrap().fail_execute = true;
    for n in 0..BREAKER_FAILURES {
        assert_eq!(
            s.search(&s.db, &job, &format!("q{n}")).await["state"],
            "uncertain"
        );
    }
    assert_eq!(s.breaker().await["state"], "open");
    // The other controller refuses too, before anything reaches Composio.
    let before = s.requests();
    let refused = s.search(&other, &job, "after").await;
    assert_eq!(
        (refused["state"].as_str(), refused["result"].as_str()),
        (Some("invalidated"), Some(BREAKER_REASON))
    );
    assert_eq!(s.requests(), before, "nothing reached the provider");
    // After the cooldown one trial call may go; another waits while it runs.
    s.sql("UPDATE relay_connector_breakers SET open_until_ms=1")
        .await;
    assert_eq!(s.breaker().await["state"], "testing");
    s.sql("UPDATE relay_connector_breakers SET probe='another-call',probe_until_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint+60000").await;
    assert_eq!(
        s.search(&other, &job, "while testing").await["result"],
        BREAKER_REASON
    );
    assert_eq!(s.requests(), before);
    // The trial fails: paused again.
    s.sql("UPDATE relay_connector_breakers SET probe=NULL,probe_until_ms=0")
        .await;
    assert_eq!(
        s.search(&other, &job, "trial one").await["state"],
        "uncertain"
    );
    assert_eq!(s.breaker().await["state"], "open");
    // The next trial succeeds: every call goes again.
    s.sql("UPDATE relay_connector_breakers SET open_until_ms=1")
        .await;
    s.fake.state.lock().unwrap().fail_execute = false;
    assert_eq!(
        s.search(&s.db, &job, "trial two").await["state"],
        "completed"
    );
    let breaker = s.breaker().await;
    assert_eq!(
        (breaker["state"].as_str(), breaker["failures"].as_i64()),
        (Some("closed"), Some(0))
    );
    assert_eq!(s.search(&other, &job, "back").await["state"], "completed");
    // A success ends a run of failures: four, a success, four more stay closed.
    s.fake.state.lock().unwrap().fail_execute = true;
    for n in 0..2 * (BREAKER_FAILURES - 1) {
        if n == BREAKER_FAILURES - 1 {
            s.fake.state.lock().unwrap().fail_execute = false;
            s.search(&s.db, &job, "between").await;
            s.fake.state.lock().unwrap().fail_execute = true;
        }
        s.search(&s.db, &job, &format!("r{n}")).await;
    }
    assert_eq!(s.breaker().await["state"], "closed");
    let refused: i64 = query(
        &s.db.db,
        "SELECT count(*) AS n FROM relay_connector_calls WHERE refused='breaker'",
        vec![],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "n")
    .unwrap();
    assert_eq!(refused, 2);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn the_rate_limit_refuses_a_busy_app_then_allows_it_a_minute_later() {
    let s = setup().await;
    let job = s.job().await;
    assert_eq!(s.search(&s.db, &job, "first").await["state"], "completed");
    let action = s.calls().await[0]["action"].as_str().unwrap().to_owned();
    // Refused calls never count toward the limit.
    let fill = |n: i64, outcome: &str, refused: Option<&str>| {
        format!(
        "INSERT INTO relay_connector_calls(id,workspace,action,agent,connection,app,provider,operation,started_ms,outcome,refused)
         SELECT gen_random_uuid()::text,'{w}','{action}','{agent}','{c}','gmail','composio','gmail.find_emails',
          (extract(epoch FROM clock_timestamp())*1000)::bigint,'{outcome}',{refused} FROM generate_series(1,{n})",
        w = s.w, agent = s.agent, c = s.connection, refused = refused.map_or("NULL".into(), |r| format!("'{r}'")))
    };
    s.sql(&fill(100, "refused", Some("rate_limit"))).await;
    s.sql(&fill(RATE_PER_MINUTE - 2, "completed", None)).await;
    assert_eq!(
        s.search(&s.db, &job, "last allowed").await["state"],
        "completed"
    );
    let before = s.requests();
    let refused = s.search(&s.db, &job, "one too many").await;
    assert_eq!(
        (refused["state"].as_str(), refused["result"].as_str()),
        (Some("invalidated"), Some(RATE_REASON))
    );
    assert_eq!(s.requests(), before, "nothing reached the provider");
    // A minute later the app may be called again.
    s.sql("UPDATE relay_connector_calls SET started_ms=started_ms-61000")
        .await;
    assert_eq!(
        s.search(&s.db, &job, "a minute later").await["state"],
        "completed"
    );
    assert_eq!(
        s.breaker().await["state"],
        "closed",
        "refusals are not provider failures"
    );
}

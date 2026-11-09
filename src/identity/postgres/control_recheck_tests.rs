//! Automatic checks of unconfirmed writes: found writes settle and continue the
//! task; other answers are recorded for a person; switched-off providers and
//! providers that can't check are left alone; two controllers ask once.
use super::super::super::*;
use super::tests::{Held, answer, request, setup};
use crate::identity::{
    connectors::{
        Account, App, Call, Capabilities, Connect, ConnectorProvider, Found, Saved, named,
    },
    control::{
        model::Operation,
        outcome_checks::{Round, run_once},
    },
};
use serde_json::json;
use std::sync::{Arc, Mutex};

/// A provider whose `find_outcome` answers what the test says, and records each ask.
#[derive(Default)]
struct Scripted {
    answer: Mutex<&'static str>,
    asked: Mutex<Vec<(String, Value)>>,
}
impl Scripted {
    fn answering(answer: &'static str) -> Arc<Self> {
        let scripted = Self::default();
        *scripted.answer.lock().unwrap() = answer;
        Arc::new(scripted)
    }
    fn asked(&self) -> Vec<(String, Value)> {
        self.asked.lock().unwrap().clone()
    }
}
#[async_trait::async_trait]
impl ConnectorProvider for Scripted {
    fn name(&self) -> &'static str {
        "mcp"
    }
    fn apps(&self, _: &str) -> Result<Vec<App>> {
        Ok(vec![])
    }
    async fn connect(&self, _: &str, _: &str, _: &str) -> Result<Connect> {
        anyhow::bail!("Not used by these tests")
    }
    async fn finish(&self, _: &str, _: &str, _: &Value) -> Result<Account> {
        anyhow::bail!("Not used by these tests")
    }
    async fn disconnect(&self, _: &str, _: &str, _: Option<&Value>) -> Result<()> {
        Ok(())
    }
    async fn prepare(&self, _: &Saved<'_>, _: &Operation) -> Result<Box<dyn Call>> {
        anyhow::bail!("Not used by these tests")
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            find_outcome: true,
            ..Capabilities::default()
        }
    }
    async fn find_outcome(
        &self,
        _: &Saved<'_>,
        _: &Operation,
        key: &str,
        arguments: &Value,
    ) -> Result<Found> {
        self.asked
            .lock()
            .unwrap()
            .push((key.to_owned(), arguments.clone()));
        Ok(match *self.answer.lock().unwrap() {
            "happened" => Found::Happened(json!({"message_id":"sent-42"})),
            "not_happened" => Found::NotHappened,
            _ => Found::Unknown,
        })
    }
}

async fn round(s: &Held, provider: &Arc<Scripted>) -> Round {
    let provider: Arc<dyn ConnectorProvider> = provider.clone();
    let lookup = move |_: &str| -> Result<Arc<dyn ConnectorProvider>> { Ok(provider.clone()) };
    run_once(&s.db, &lookup).await
}

/// Moves the action, or its next check, into the past.
async fn age(s: &Held, sql: &str, id: &str) {
    execute(&s.db.db, sql, vec![id.into()]).await.unwrap();
}
const BECAME_UNCONFIRMED: &str = "UPDATE relay_control_actions SET updated=updated-61 WHERE id=$1";
const NEXT_CHECK_DUE: &str = "UPDATE relay_control_outcome_checks SET next_at=0 WHERE action=$1";

async fn checks(s: &Held, id: &str) -> Value {
    let details = s.db.approval_details(&s.owner, &s.w, id).await.unwrap();
    details["settle"]["checks"].clone()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_check_that_finds_the_write_settles_it_and_the_waiting_task_continues() {
    let s = setup("ask", false).await;
    let id = s.unconfirmed("one", &s.owner).await;
    let provider = Scripted::answering("happened");
    // The first check is about a minute after the provider didn't confirm it.
    assert_eq!(round(&s, &provider).await, Round::default());
    age(&s, BECAME_UNCONFIRMED, &id).await;
    let done = round(&s, &provider).await;
    assert_eq!(
        done,
        Round {
            checked: 1,
            settled: 1
        }
    );
    // Asked with the action's id, the key its call was given, and its exact fields.
    assert_eq!(
        provider.asked(),
        vec![(id.clone(), request(&s.connection, "one").arguments)]
    );
    let details = s.db.approval_details(&s.owner, &s.w, &id).await.unwrap();
    assert_eq!(details["state"], "completed");
    assert_eq!(details["settled"]["by"], Value::Null);
    assert_eq!(details["settled"]["by_name"], "Cuckoo");
    assert_eq!(details["result"]["found"]["message_id"], "sent-42");
    // A person who then agrees gets the saved answer; disagreeing conflicts.
    s.db.control_settle(&s.manager, &s.w, &id, &answer(true, ""))
        .await
        .unwrap();
    assert!(
        s.db.control_settle(&s.manager, &s.w, &id, &answer(false, ""))
            .await
            .is_err()
    );
    assert_eq!(s.job_state().await, "queued");
    let resumed = s.db.claim_job().await.unwrap().unwrap();
    let saved = s.db.task_continuation(&resumed).await.unwrap().unwrap();
    let data: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(data["actions"][0]["state"], "completed");
    let other =
        s.db.control_propose(&resumed, &request(&s.connection, "two"))
            .await
            .unwrap();
    assert_eq!(other.action["state"], "pending", "the hold is released");
    let audited = query(
        &s.db.db,
        "SELECT actor FROM relay_audit WHERE action='control_action_settled' AND target=$1",
        vec![id.clone().into()],
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(audited.try_get::<String>("", "actor").unwrap(), "system");
    assert_eq!(round(&s, &provider).await, Round::default());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn other_answers_are_recorded_for_a_person_at_one_five_and_thirty_minutes() {
    let s = setup("allow", false).await;
    let id = s.unconfirmed("one", &s.owner).await;
    age(&s, BECAME_UNCONFIRMED, &id).await;
    let provider = Scripted::answering("unknown");
    let checked = Round {
        checked: 1,
        settled: 0,
    };
    assert_eq!(round(&s, &provider).await, checked);
    assert_eq!(checks(&s, &id).await[0]["result"], "unknown");
    // Not again until five minutes after it became unconfirmed.
    assert_eq!(round(&s, &provider).await, Round::default());
    let schedule = query(
        &s.db.db,
        "SELECT next_at-since AS after FROM relay_control_outcome_checks WHERE action=$1",
        vec![id.clone().into()],
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(schedule.try_get::<i64>("", "after").unwrap(), 300);
    *provider.answer.lock().unwrap() = "not_happened";
    age(&s, NEXT_CHECK_DUE, &id).await;
    assert_eq!(round(&s, &provider).await, checked);
    age(&s, NEXT_CHECK_DUE, &id).await;
    assert_eq!(round(&s, &provider).await, checked);
    let results: Vec<_> = checks(&s, &id)
        .await
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["result"].clone())
        .collect();
    assert_eq!(results, ["unknown", "not_happened", "not_happened"]);
    // Three checks at most; the write stays held until a person answers.
    age(&s, NEXT_CHECK_DUE, &id).await;
    assert_eq!(round(&s, &provider).await, Round::default());
    assert_eq!(provider.asked().len(), 3);
    let details = s.db.approval_details(&s.owner, &s.w, &id).await.unwrap();
    assert_eq!(details["state"], "uncertain");
    assert_eq!(details["settle"]["can_settle"], true);
    let settled =
        s.db.control_settle(&s.owner, &s.w, &id, &answer(false, ""))
            .await
            .unwrap();
    assert_eq!(settled["state"], "not_sent");
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn switched_off_providers_wait_and_two_controllers_ask_once() {
    let s = setup("allow", false).await;
    let id = s.unconfirmed("one", &s.owner).await;
    age(&s, BECAME_UNCONFIRMED, &id).await;
    let provider = Scripted::answering("unknown");
    s.db.connector_provider_set(&s.owner, "mcp", false, "Provider incident")
        .await
        .unwrap();
    assert_eq!(round(&s, &provider).await, Round::default());
    assert!(provider.asked().is_empty());
    assert_eq!(checks(&s, &id).await, json!([]));
    s.db.connector_provider_set(&s.owner, "mcp", true, "")
        .await
        .unwrap();
    let (a, b) = tokio::join!(round(&s, &provider), round(&s, &provider));
    assert_eq!(a.checked + b.checked, 1);
    assert_eq!(provider.asked().len(), 1);
    assert_eq!(checks(&s, &id).await.as_array().unwrap().len(), 1);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn providers_that_cannot_find_outcomes_are_not_asked_and_record_nothing() {
    let s = setup("allow", false).await;
    let id = s.unconfirmed("one", &s.owner).await;
    age(&s, BECAME_UNCONFIRMED, &id).await;
    // Operator MCP servers (like Composio) keep the contract's default: Unknown.
    let mcp = named("mcp").unwrap();
    assert!(!mcp.capabilities().find_outcome);
    let lookup = move |_: &str| -> Result<Arc<dyn ConnectorProvider>> { Ok(mcp.clone()) };
    assert_eq!(run_once(&s.db, &lookup).await, Round::default());
    assert_eq!(checks(&s, &id).await, json!([]));
    let provider = Scripted::answering("happened");
    age(&s, NEXT_CHECK_DUE, &id).await;
    assert_eq!(round(&s, &provider).await, Round::default());
    assert!(provider.asked().is_empty());
}

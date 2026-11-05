//! Agents propose rules that stay off until an admin reviews them; trials record
//! without enforcing; replay shows what a draft would have done.
use super::super::super::super::tests::{join, message, user, workspace};
use super::super::super::super::*;
use super::super::GuardRule;
use super::RuleProposal;
use crate::identity::{chat::model::Job, control::model::ActionRequest};
use serde_json::json;

async fn setup() -> (PgStore, User, String, String, Job) {
    let db = super::super::super::super::tests::fixture().await;
    let owner = user(&db, "proposal-owner").await;
    let w = workspace(&db, &owner).await;
    let (chat, agent) =
        super::super::super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    let connection = uuid::Uuid::new_v4().to_string();
    let definition = json!({"id":"shop","name":"Shop","operations":[{"id":"refund.create","title":"Refund an order","tool":"refund","effect":"write","schema":{"type":"object","properties":{"order":{"type":"string"},"amount":{"type":"number"}},"required":["order"],"additionalProperties":false}}]});
    db.control_system(
        &owner,
        &w,
        &crate::identity::control::connections_model::SystemApproval {
            server: "shop".into(),
            approved: true,
            revision: 0,
        },
    )
    .await
    .unwrap();
    db.control_connect(&owner, &w, &agent, &connection, definition, "reviewed")
        .await
        .unwrap();
    super::super::super::super::tool_test_support::grant(
        &db,
        &owner,
        &w,
        &agent,
        &connection,
        "refund.create",
        "allow",
    )
    .await;
    db.chat_send(
        &owner,
        &w,
        &chat,
        &message("Refund the customer", Some(&agent)),
    )
    .await
    .unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    (db, owner, w, connection, job)
}
fn refund(connection: &str, order: &str, amount: f64) -> ActionRequest {
    ActionRequest {
        connection: connection.into(),
        operation: "refund.create".into(),
        arguments: json!({"order": order, "amount": amount}),
    }
}
fn proposal(value: Value) -> RuleProposal {
    RuleProposal {
        words: "Refunds over 2000 need my approval".into(),
        operation: Some("refund.create".into()),
        connection: None,
        meaning: None,
        kind: "field".into(),
        source: None,
        field: "amount".into(),
        comparator: ">".into(),
        value,
        effect: "ask".into(),
    }
}
fn rule(kind: &str, comparator: &str, value: Value, effect: &str) -> GuardRule {
    GuardRule {
        id: uuid::Uuid::new_v4().to_string(),
        operation: Some("refund.create".into()),
        connection: None,
        meaning: None,
        source: None,
        kind: kind.into(),
        field: "amount".into(),
        comparator: comparator.into(),
        value,
        effect: effect.into(),
        note: String::new(),
        enabled: true,
        revision: 0,
        trial: false,
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn an_agents_proposal_stays_off_until_an_admin_turns_it_on() {
    let (db, owner, w, connection, job) = setup().await;
    let proposed = db
        .propose_guard_rule(&job, &proposal(json!(2000)))
        .await
        .unwrap();
    assert_eq!(proposed["state"], "proposed");
    assert_eq!(
        proposed["summary"],
        "Refund an order: if amount is over 2000, ask first."
    );
    assert_eq!(
        db.propose_guard_rule(
            &job,
            &RuleProposal {
                field: "missing".into(),
                ..proposal(json!(1))
            }
        )
        .await
        .unwrap()["state"],
        "invalid"
    );
    let id = proposed["rule"].as_str().unwrap().to_owned();
    let listed = db.guard_rules(&owner, &w).await.unwrap()["rules"][0].clone();
    assert_eq!(
        (
            listed["reviewed"].as_bool(),
            listed["enabled"].as_bool(),
            listed["proposal"].as_str()
        ),
        (
            Some(false),
            Some(false),
            Some("Refunds over 2000 need my approval")
        )
    );
    assert!(listed["proposed_by_name"].is_string());
    // Not in force yet.
    let first = db
        .control_propose(&job, &refund(&connection, "A", 2500.0))
        .await
        .unwrap();
    assert_eq!(first.action["state"], "executing");
    db.control_finish(&w, &first.dispatch.unwrap(), json!({"ok":true}), true)
        .await
        .unwrap();
    // A member can't dismiss it; the admin turns it on by saving it.
    let member = user(&db, "proposal-member").await;
    join(&db, &owner, &w, &member, "Member").await;
    assert!(db.guard_rule_dismiss(&member, &w, &id).await.is_err());
    db.guard_rule_save(
        &owner,
        &w,
        &GuardRule {
            id: id.clone(),
            revision: 1,
            ..rule("field", ">", json!(2000), "ask")
        },
    )
    .await
    .unwrap();
    assert_eq!(
        db.control_propose(&job, &refund(&connection, "B", 2500.0))
            .await
            .unwrap()
            .action["state"],
        "pending"
    );
    assert!(
        db.guard_rule_dismiss(&owner, &w, &id).await.is_err(),
        "a reviewed rule isn't a proposal any more"
    );
    // Another proposal can be dismissed; it's gone and audited.
    let other = db
        .propose_guard_rule(&job, &proposal(json!(5000)))
        .await
        .unwrap()["rule"]
        .as_str()
        .unwrap()
        .to_owned();
    db.guard_rule_dismiss(&owner, &w, &other).await.unwrap();
    assert_eq!(
        db.guard_rules(&owner, &w).await.unwrap()["rules"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let said: String = query(
        &db.db,
        "SELECT summary FROM relay_audit WHERE workspace=$1 AND action='guard_rule_dismissed'",
        vec![w.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "summary")
    .unwrap();
    assert!(said.contains("Refunds over 2000 need my approval"));
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_trial_week_records_what_a_rule_would_do_and_then_enforces() {
    let (db, owner, w, connection, job) = setup().await;
    let saved = db
        .guard_rule_save(
            &owner,
            &w,
            &GuardRule {
                trial: true,
                ..rule("field", ">", json!(2000), "block")
            },
        )
        .await
        .unwrap();
    assert!(saved["trial_until_ms"].as_i64().unwrap() > 0);
    let tried = db
        .control_propose(&job, &refund(&connection, "T", 2500.0))
        .await
        .unwrap();
    assert_eq!(tried.action["state"], "executing", "a trial never blocks");
    assert_eq!(tried.action["guard"]["trial"][0]["rule"], saved["id"]);
    assert_eq!(tried.action["guard"]["trial"][0]["effect"], "block");
    assert!(tried.action["guard"]["effect"].is_null());
    db.control_finish(&w, &tried.dispatch.unwrap(), json!({"ok":true}), true)
        .await
        .unwrap();
    assert_eq!(
        db.guard_rules(&owner, &w).await.unwrap()["rules"][0]["trial_matches"],
        1
    );
    execute(
        &db.db,
        "UPDATE relay_guard_rules SET trial_until_ms=1 WHERE id=$1",
        vec![saved["id"].as_str().unwrap().into()],
    )
    .await
    .unwrap();
    assert_eq!(
        db.control_propose(&job, &refund(&connection, "U", 2500.0))
            .await
            .unwrap()
            .action["state"],
        "denied"
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn replay_shows_what_a_draft_would_have_done_to_the_last_thirty_days() {
    let (db, owner, w, connection, job) = setup().await;
    for (order, amount) in [("R1", 500.0), ("R2", 2500.0), ("R3", 3000.0)] {
        let p = db
            .control_propose(&job, &refund(&connection, order, amount))
            .await
            .unwrap();
        db.control_finish(&w, &p.dispatch.unwrap(), json!({"ok":true}), true)
            .await
            .unwrap();
    }
    let field = db
        .guard_rule_replay(&owner, &w, &rule("field", ">", json!(2000), "ask"))
        .await
        .unwrap();
    assert_eq!(
        (
            field["examined"].as_i64(),
            field["would_hold"].as_i64(),
            field["would_block"].as_i64()
        ),
        (Some(3), Some(2), Some(0))
    );
    assert_eq!(
        field["summary"],
        "Refund an order: if amount is over 2000, ask first."
    );
    let total = db
        .guard_rule_replay(&owner, &w, &rule("daily_total", ">", json!(3000), "block"))
        .await
        .unwrap();
    assert_eq!(total["would_block"], 1);
    assert_eq!(total["examples"][0]["arguments"]["order"], "R3");
    let member = user(&db, "replay-member").await;
    join(&db, &owner, &w, &member, "Member").await;
    assert!(
        db.guard_rule_replay(&member, &w, &rule("field", ">", json!(1), "ask"))
            .await
            .is_err()
    );
    assert!(
        db.guard_rules(&owner, &w).await.unwrap()["rules"]
            .as_array()
            .unwrap()
            .is_empty(),
        "replay saves nothing"
    );
}

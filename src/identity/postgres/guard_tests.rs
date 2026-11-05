//! Company rules hold or block exact requests, reserve daily totals and fail safe.
use super::super::super::tests::{join, message, user, workspace};
use super::super::super::*;
use super::GuardRule;
use crate::identity::{chat::model::Job, control::model::ActionRequest};
use serde_json::json;

/// An agent with a refunds account: `refund.create {order, amount, reason?}`.
async fn setup(mode: &str) -> (PgStore, User, String, String, Job) {
    let db = super::super::super::tests::fixture().await;
    let owner = user(&db, "guard-owner").await;
    let w = workspace(&db, &owner).await;
    let (chat, agent) = super::super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    let connection = uuid::Uuid::new_v4().to_string();
    let definition = json!({"id":"shop","name":"Shop","operations":[{"id":"refund.create","title":"Refund an order","tool":"refund","effect":"write","schema":{"type":"object","properties":{"order":{"type":"string"},"amount":{"type":"number"},"reason":{"type":"string"}},"required":["order"],"additionalProperties":false}}]});
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
    super::super::super::tool_test_support::grant(
        &db,
        &owner,
        &w,
        &agent,
        &connection,
        "refund.create",
        mode,
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
fn refund(connection: &str, order: &str, amount: Value) -> ActionRequest {
    let mut arguments = json!({"order": order});
    if !amount.is_null() {
        arguments["amount"] = amount;
    }
    ActionRequest {
        connection: connection.into(),
        operation: "refund.create".into(),
        arguments,
    }
}
fn rule(kind: &str, comparator: &str, value: Value, effect: &str) -> GuardRule {
    GuardRule {
        id: uuid::Uuid::new_v4().to_string(),
        operation: "refund.create".into(),
        connection: None,
        kind: kind.into(),
        field: "amount".into(),
        comparator: comparator.into(),
        value,
        effect: effect.into(),
        note: String::new(),
        enabled: true,
        revision: 0,
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_field_rule_holds_large_refunds_and_records_its_facts() {
    let (db, owner, w, connection, job) = setup("allow").await;
    let saved = db
        .guard_rule_save(&owner, &w, &rule("field", ">", json!(2000), "ask"))
        .await
        .unwrap();
    assert_eq!(
        saved["summary"],
        "Refund an order: if amount is over 2000, ask first."
    );
    // The agent sees the rule before it tries.
    let ops = db.control_operations(&job).await.unwrap();
    assert_eq!(ops["operations"][0]["company_rules"][0], saved["summary"]);
    let small = db
        .control_propose(&job, &refund(&connection, "A-1", json!(500)))
        .await
        .unwrap();
    assert_eq!(small.action["state"], "executing");
    assert!(small.action["guard"].is_null());
    db.control_finish(&w, &small.dispatch.unwrap(), json!({"ok":true}), true)
        .await
        .unwrap();
    let large = db
        .control_propose(&job, &refund(&connection, "A-2", json!(2500)))
        .await
        .unwrap();
    assert!(large.dispatch.is_none(), "held for a person, nothing sent");
    assert_eq!(large.action["state"], "pending");
    let guard = &large.action["guard"];
    assert_eq!(guard["effect"], "ask");
    assert_eq!(guard["rule"], saved["id"]);
    assert_eq!(guard["facts"]["value"], 2500);
    assert_eq!(guard["summary"], saved["summary"]);
    // The approval shows the same decision record.
    let id = large.action["id"].as_str().unwrap();
    assert_eq!(
        db.approval_details(&owner, &w, id).await.unwrap()["guard"]["rule"],
        saved["id"]
    );
    // The agent may do this action alone, yet a held request can still be decided: once.
    let details = db.approval_details(&owner, &w, id).await.unwrap();
    assert_eq!(details["can_decide"], true);
    let decided = db
        .control_decide(
            &owner,
            &w,
            &job.agent,
            id,
            &crate::identity::control::model::Decision {
                id: uuid::Uuid::new_v4().to_string(),
                decision: "approve".into(),
            },
        )
        .await
        .unwrap();
    assert!(
        decided.dispatch.is_some(),
        "approving sends the exact fields"
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn block_wins_missing_facts_hold_and_rules_never_loosen() {
    let (db, owner, w, connection, job) = setup("ask").await;
    db.guard_rule_save(&owner, &w, &rule("field", ">", json!(2000), "ask"))
        .await
        .unwrap();
    db.guard_rule_save(&owner, &w, &rule("field", ">=", json!(10000), "block"))
        .await
        .unwrap();
    let huge = db
        .control_propose(&job, &refund(&connection, "B-1", json!(12000)))
        .await
        .unwrap();
    assert_eq!(huge.action["state"], "denied");
    assert_eq!(huge.action["guard"]["effect"], "block");
    assert_eq!(huge.action["guard"]["matched"], 2);
    // No amount: the facts can't decide, so a person decides; even a block rule asks.
    let unknown = db
        .control_propose(&job, &refund(&connection, "B-2", Value::Null))
        .await
        .unwrap();
    assert_eq!(unknown.action["state"], "pending");
    assert_eq!(unknown.action["guard"]["known"], false);
    // Under every limit the agent's own rule (ask) still applies: the Guard never loosens.
    let small = db
        .control_propose(&job, &refund(&connection, "B-3", json!(100)))
        .await
        .unwrap();
    assert_eq!(small.action["state"], "pending");
    assert!(small.action["guard"].is_null());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn daily_totals_reserve_waiting_requests_even_in_parallel() {
    let (db, owner, w, connection, job) = setup("ask").await;
    let limit = db
        .guard_rule_save(&owner, &w, &rule("daily_total", ">", json!(1000), "block"))
        .await
        .unwrap();
    assert_eq!(
        limit["summary"],
        "Refund an order: if today's total amount would go over 1000, block it."
    );
    let mut work = tokio::task::JoinSet::new();
    for i in 0..10 {
        let (db, job, request) = (
            db.clone(),
            job.clone(),
            refund(&connection, &format!("C-{i}"), json!(300)),
        );
        work.spawn(async move { db.control_propose(&job, &request).await.unwrap().action });
    }
    let mut states = vec![];
    while let Some(action) = work.join_next().await {
        states.push(action.unwrap()["state"].as_str().unwrap().to_owned());
    }
    // 3 × 300 fit under 1000 and wait for approval; they reserve the total.
    assert_eq!(states.iter().filter(|s| *s == "pending").count(), 3);
    assert_eq!(states.iter().filter(|s| *s == "denied").count(), 7);
    let blocked: Value = query(
        &db.db,
        "SELECT guard FROM relay_control_actions WHERE workspace=$1 AND state='denied' LIMIT 1",
        vec![w.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "guard")
    .unwrap();
    assert_eq!(blocked["facts"]["today_before"], 900.0);
    assert_eq!(blocked["facts"]["today_after"], 1200.0);
    // Rejecting one frees its reservation.
    let waiting: String = query(
        &db.db,
        "SELECT id FROM relay_control_actions WHERE workspace=$1 AND state='pending' LIMIT 1",
        vec![w.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "id")
    .unwrap();
    execute(
        &db.db,
        "UPDATE relay_control_actions SET state='rejected' WHERE id=$1",
        vec![waiting.into()],
    )
    .await
    .unwrap();
    let again = db
        .control_propose(&job, &refund(&connection, "C-next", json!(100)))
        .await
        .unwrap();
    assert_eq!(again.action["state"], "pending");
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn only_admins_change_rules_and_rules_are_checked_against_the_action() {
    let (db, owner, w, _, _) = setup("allow").await;
    let member = user(&db, "guard-member").await;
    join(&db, &owner, &w, &member, "Member").await;
    let mut input = rule("field", ">", json!(2000), "ask");
    assert!(db.guard_rule_save(&member, &w, &input).await.is_err());
    // Members read the rules with their plain-words summary; they can't manage them.
    db.guard_rule_save(&owner, &w, &input).await.unwrap();
    let seen = db.guard_rules(&member, &w).await.unwrap();
    assert_eq!(seen["can_manage"], false);
    assert_eq!(
        seen["rules"][0]["summary"],
        "Refund an order: if amount is over 2000, ask first."
    );
    assert_eq!(seen["actions"][0]["operation"], "refund.create");
    // A stale revision, an unknown field, a number comparison on text, a text total.
    assert!(
        db.guard_rule_save(&owner, &w, &input).await.is_err(),
        "revision 0 is stale now"
    );
    input.revision = 1;
    input.enabled = false;
    assert_eq!(
        db.guard_rule_save(&owner, &w, &input).await.unwrap()["revision"],
        2
    );
    let mut bad = rule("field", ">", json!(1), "ask");
    bad.field = "missing".into();
    assert!(db.guard_rule_save(&owner, &w, &bad).await.is_err());
    let mut text = rule("field", ">", json!(1), "ask");
    text.field = "reason".into();
    assert!(db.guard_rule_save(&owner, &w, &text).await.is_err());
    let mut equals = rule("field", "=", json!("fraud"), "block");
    equals.field = "reason".into();
    assert_eq!(
        db.guard_rule_save(&owner, &w, &equals).await.unwrap()["summary"],
        "Refund an order: if reason is \"fraud\", block it."
    );
    let mut total = rule("daily_total", "<", json!(1), "ask");
    assert!(db.guard_rule_save(&owner, &w, &total).await.is_err());
    total.comparator = ">=".into();
    total.field = "reason".into();
    assert!(db.guard_rule_save(&owner, &w, &total).await.is_err());
    // Every change is in the audit log in plain words.
    let said: String = query(&db.db, "SELECT summary FROM relay_audit WHERE workspace=$1 AND action='guard_rule_saved' ORDER BY at DESC, summary LIMIT 1", vec![w.clone().into()]).await.unwrap().unwrap().try_get("", "summary").unwrap();
    assert!(said.contains("company rule"));
}

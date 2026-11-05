//! Facts the agent didn't send: values from an earlier read in the same task, and
//! "read before you change" rules.
use super::super::super::tests::{message, user, workspace};
use super::super::super::*;
use super::GuardRule;
use crate::identity::{chat::model::Job, control::model::ActionRequest};
use serde_json::json;

fn op(id: &str, title: &str, tool: &str, effect: &str, extra: Value) -> Value {
    let mut properties = json!({"order":{"type":"string"}});
    properties
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    json!({"id":id,"title":title,"tool":tool,"effect":effect,"schema":{"type":"object","properties":properties,"required":["order"],"additionalProperties":false}})
}

/// A shop whose refunds carry only an order ID; its total is in `Find an order`.
async fn setup() -> (PgStore, User, String, String, Job) {
    let db = super::super::super::tests::fixture().await;
    let owner = user(&db, "facts-owner").await;
    let w = workspace(&db, &owner).await;
    let (chat, agent) = super::super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    let connection = uuid::Uuid::new_v4().to_string();
    let definition = json!({"id":"shop","name":"Shop","operations":[
        op("order.read","Find an order","read_order","read",json!({})),
        op("refund.create","Refund an order","refund","write",json!({"reason":{"type":"string"}}))]});
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
    for operation in ["order.read", "refund.create"] {
        super::super::super::tool_test_support::grant(
            &db,
            &owner,
            &w,
            &agent,
            &connection,
            operation,
            "allow",
        )
        .await;
    }
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
fn request(connection: &str, operation: &str, order: &str) -> ActionRequest {
    ActionRequest {
        connection: connection.into(),
        operation: operation.into(),
        arguments: json!({"order": order}),
    }
}
/// The agent reads an order; the provider answers with its total.
async fn read(db: &PgStore, w: &str, job: &Job, connection: &str, order: &str, total: f64) {
    let proposed = db
        .control_propose(job, &request(connection, "order.read", order))
        .await
        .unwrap();
    db.control_finish(
        w,
        &proposed.dispatch.unwrap(),
        json!({"structuredContent":{"order":order,"total":total}}),
        true,
    )
    .await
    .unwrap();
}
fn rule(
    kind: &str,
    source: Value,
    field: &str,
    comparator: &str,
    value: Value,
    effect: &str,
) -> GuardRule {
    GuardRule {
        id: uuid::Uuid::new_v4().to_string(),
        operation: Some("refund.create".into()),
        connection: None,
        meaning: None,
        source: Some(source),
        kind: kind.into(),
        field: field.into(),
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
async fn a_rule_uses_the_total_cuckoo_read_not_one_the_agent_sends() {
    let (db, owner, w, connection, job) = setup().await;
    let saved = db
        .guard_rule_save(
            &owner,
            &w,
            &rule(
                "field",
                json!({"operation":"order.read","match":"order","path":"total"}),
                "total",
                ">",
                json!(2000),
                "ask",
            ),
        )
        .await
        .unwrap();
    assert_eq!(
        saved["summary"],
        "Refund an order: if total from Find an order is over 2000, ask first."
    );
    // Nothing read yet: Cuckoo can't check the total, so a person decides.
    let unread = db
        .control_propose(&job, &request(&connection, "refund.create", "A"))
        .await
        .unwrap();
    assert_eq!(unread.action["state"], "pending");
    assert_eq!(unread.action["guard"]["known"], false);
    // A small order runs; a large one waits, with where the fact came from.
    read(&db, &w, &job, &connection, "B", 500.0).await;
    let small = db
        .control_propose(&job, &request(&connection, "refund.create", "B"))
        .await
        .unwrap();
    assert_eq!(small.action["state"], "executing");
    db.control_finish(&w, &small.dispatch.unwrap(), json!({"ok":true}), true)
        .await
        .unwrap();
    read(&db, &w, &job, &connection, "C", 2450.0).await;
    let large = db
        .control_propose(&job, &request(&connection, "refund.create", "C"))
        .await
        .unwrap();
    assert_eq!(large.action["state"], "pending");
    assert_eq!(large.action["guard"]["facts"]["value"], 2450.0);
    assert_eq!(
        large.action["guard"]["facts"]["from"]["operation"],
        "order.read"
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn read_first_rules_need_the_same_record_read_in_this_task() {
    let (db, owner, w, connection, job) = setup().await;
    let saved = db
        .guard_rule_save(
            &owner,
            &w,
            &rule(
                "read_first",
                json!({"operation":"order.read","match":"order"}),
                "",
                "=",
                json!(true),
                "block",
            ),
        )
        .await
        .unwrap();
    assert_eq!(
        saved["summary"],
        "Refund an order: unless Find an order for the same order ran first in this task, block it."
    );
    let blind = db
        .control_propose(&job, &request(&connection, "refund.create", "D"))
        .await
        .unwrap();
    assert_eq!(blind.action["state"], "denied");
    assert_eq!(blind.action["guard"]["facts"]["found"], false);
    read(&db, &w, &job, &connection, "E", 10.0).await;
    // Reading a different order doesn't count.
    assert_eq!(
        db.control_propose(&job, &request(&connection, "refund.create", "F"))
            .await
            .unwrap()
            .action["state"],
        "denied"
    );
    assert_eq!(
        db.control_propose(&job, &request(&connection, "refund.create", "E"))
            .await
            .unwrap()
            .action["state"],
        "executing"
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn sources_must_be_a_read_on_the_same_account_sharing_the_matched_field() {
    let (db, owner, w, _, _) = setup().await;
    let bad = [
        rule(
            "field",
            json!({"operation":"refund.create","match":"order","path":"total"}),
            "total",
            ">",
            json!(1),
            "ask",
        ),
        rule(
            "field",
            json!({"operation":"order.read","match":"reason","path":"total"}),
            "total",
            ">",
            json!(1),
            "ask",
        ),
        rule(
            "field",
            json!({"operation":"order.read","match":"order","path":"total"}),
            "amount",
            ">",
            json!(1),
            "ask",
        ),
        rule(
            "field",
            json!({"operation":"order.read","match":"order","path":"total"}),
            "total",
            ">",
            json!("x"),
            "ask",
        ),
        rule(
            "read_first",
            json!({"operation":"order.read","match":"order","path":"total"}),
            "",
            "=",
            json!(true),
            "ask",
        ),
        rule(
            "time",
            json!({"operation":"order.read","match":"order"}),
            "",
            "=",
            json!({"from":"00:00","to":"00:00","days":[]}),
            "ask",
        ),
    ];
    for input in bad {
        assert!(db.guard_rule_save(&owner, &w, &input).await.is_err());
    }
}

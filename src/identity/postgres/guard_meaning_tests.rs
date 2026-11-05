//! What actions do (meanings), safe defaults from them, rules for every action of one
//! meaning, and time-of-day rules.
use super::super::super::tests::{join, message, user, workspace};
use super::super::super::*;
use super::{ActionMeaning, GuardRule};
use crate::identity::{chat::model::Job, control::model::ActionRequest};
use serde_json::json;

fn op(id: &str, title: &str, tool: &str, effect: &str) -> Value {
    json!({"id":id,"title":title,"tool":tool,"effect":effect,"schema":{"type":"object","properties":{"order":{"type":"string"},"amount":{"type":"number"}},"required":["order"],"additionalProperties":false}})
}

/// An agent using a shop with a read, a change, a refund, a payment and a delete.
/// Only the reading action has an admin rule (allow); the rest use the defaults,
/// except refunds and payments, which the admin allows so company rules decide.
async fn setup() -> (PgStore, User, String, String, Job) {
    let db = super::super::super::tests::fixture().await;
    let owner = user(&db, "meaning-owner").await;
    let w = workspace(&db, &owner).await;
    let (chat, agent) = super::super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    let connection = uuid::Uuid::new_v4().to_string();
    let definition = json!({"id":"shop","name":"Shop","operations":[
        op("order.read","Find an order","read_order","read"), op("order.update","Update an order","update_order","write"),
        op("refund.create","Refund an order","refund","write"), op("payment.create","Pay a supplier","pay_supplier","write"),
        op("order.delete","Delete an order","delete_order","write")]});
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
    for operation in ["refund.create", "payment.create"] {
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
    db.chat_send(&owner, &w, &chat, &message("Handle the shop", Some(&agent)))
        .await
        .unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    (db, owner, w, connection, job)
}
fn request(connection: &str, operation: &str, order: &str, amount: f64) -> ActionRequest {
    ActionRequest {
        connection: connection.into(),
        operation: operation.into(),
        arguments: json!({"order": order, "amount": amount}),
    }
}
fn rule(meaning: &str, kind: &str, comparator: &str, value: Value, effect: &str) -> GuardRule {
    GuardRule {
        id: uuid::Uuid::new_v4().to_string(),
        operation: None,
        connection: None,
        meaning: Some(meaning.into()),
        kind: kind.into(),
        field: if kind == "time" {
            String::new()
        } else {
            "amount".into()
        },
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
async fn meanings_are_suggested_deletes_are_blocked_by_default_and_admins_review_them() {
    let (db, owner, w, connection, job) = setup().await;
    let safety =
        db.workspace_connections(&owner, &w).await.unwrap()["safety"][0]["operations"].clone();
    let by_id = |id: &str| {
        safety
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["id"] == id)
            .unwrap()
            .clone()
    };
    assert_eq!(by_id("order.read")["meaning"], "read");
    assert_eq!(by_id("order.update")["meaning"], "change");
    assert_eq!(by_id("refund.create")["meaning"], "money");
    assert_eq!(by_id("payment.create")["meaning"], "money");
    let delete = by_id("order.delete");
    assert_eq!(
        (
            delete["meaning"].as_str(),
            delete["mode"].as_str(),
            delete["meaning_reviewed"].as_bool()
        ),
        (Some("delete"), Some("block"), Some(false))
    );
    assert_eq!(by_id("order.update")["mode"], "ask");
    // The agent can't use a delete it was never explicitly allowed.
    let denied = db
        .control_propose(&job, &request(&connection, "order.delete", "D-1", 0.0))
        .await;
    assert!(
        denied
            .map(|p| p.action["state"] == "denied")
            .unwrap_or(true)
    );
    // An admin reviews: "Update an order" moves money here. Members can't; reading
    // actions stay reading; a stale revision is refused; it's in the audit log.
    let review = |meaning: &str, revision: i64| ActionMeaning {
        connection: connection.clone(),
        operation: "order.update".into(),
        meaning: meaning.into(),
        revision,
    };
    let member = user(&db, "meaning-member").await;
    join(&db, &owner, &w, &member, "Member").await;
    assert!(
        db.action_meaning_save(&member, &w, &review("money", 0))
            .await
            .is_err()
    );
    assert!(
        db.action_meaning_save(&owner, &w, &review("read", 0))
            .await
            .is_err()
    );
    assert_eq!(
        db.action_meaning_save(&owner, &w, &review("money", 0))
            .await
            .unwrap()["revision"],
        1
    );
    assert!(
        db.action_meaning_save(&owner, &w, &review("delete", 0))
            .await
            .is_err(),
        "stale"
    );
    let read_as_change = ActionMeaning {
        connection: connection.clone(),
        operation: "order.read".into(),
        meaning: "change".into(),
        revision: 0,
    };
    assert!(
        db.action_meaning_save(&owner, &w, &read_as_change)
            .await
            .is_err()
    );
    let safety =
        db.workspace_connections(&owner, &w).await.unwrap()["safety"][0]["operations"].clone();
    let update = safety
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["id"] == "order.update")
        .unwrap()
        .clone();
    assert_eq!(
        (
            update["meaning"].as_str(),
            update["meaning_reviewed"].as_bool()
        ),
        (Some("money"), Some(true))
    );
    let said: String = query(
        &db.db,
        "SELECT summary FROM relay_audit WHERE workspace=$1 AND action='action_meaning_saved'",
        vec![w.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "summary")
    .unwrap();
    assert_eq!(said, "Reviewed Update an order as money action");
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn one_rule_covers_every_money_action_and_totals_add_up_across_them() {
    let (db, owner, w, connection, job) = setup().await;
    let saved = db
        .guard_rule_save(
            &owner,
            &w,
            &rule("money", "daily_total", ">", json!(1000), "block"),
        )
        .await
        .unwrap();
    assert_eq!(
        saved["summary"],
        "Any money action: if today's total amount would go over 1000, block it."
    );
    let ops = db.control_operations(&job).await.unwrap();
    let refund = ops["operations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["operation"] == "refund.create")
        .unwrap();
    assert_eq!(refund["company_rules"][0], saved["summary"]);
    let first = db
        .control_propose(&job, &request(&connection, "refund.create", "R-1", 600.0))
        .await
        .unwrap();
    assert_eq!(first.action["state"], "executing");
    assert_eq!(first.action["meaning"], "money");
    db.control_finish(&w, &first.dispatch.unwrap(), json!({"ok":true}), true)
        .await
        .unwrap();
    // A different money action shares the same day's total.
    let second = db
        .control_propose(&job, &request(&connection, "payment.create", "P-1", 600.0))
        .await
        .unwrap();
    assert_eq!(second.action["state"], "denied");
    assert_eq!(second.action["guard"]["facts"]["today_before"], 600.0);
    let small = db
        .control_propose(&job, &request(&connection, "payment.create", "P-2", 300.0))
        .await
        .unwrap();
    assert_eq!(small.action["state"], "executing");
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn time_rules_hold_requests_in_their_window_in_the_workspace_time_zone() {
    let (db, owner, w, connection, job) = setup().await;
    execute(
        &db.db,
        "UPDATE relay_workspaces SET timezone='Asia/Kolkata' WHERE id=$1",
        vec![w.clone().into()],
    )
    .await
    .unwrap();
    let today: i32 = query(
        &db.db,
        "SELECT extract(isodow FROM clock_timestamp() AT TIME ZONE 'Asia/Kolkata')::int AS d",
        vec![],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "d")
    .unwrap();
    let other = today % 7 + 1;
    // Another day only: nothing happens today.
    let elsewhere = db
        .guard_rule_save(
            &owner,
            &w,
            &rule(
                "money",
                "time",
                "=",
                json!({"from":"00:00","to":"00:00","days":[other]}),
                "block",
            ),
        )
        .await
        .unwrap();
    let ok = db
        .control_propose(&job, &request(&connection, "refund.create", "T-1", 10.0))
        .await
        .unwrap();
    assert_eq!(ok.action["state"], "executing");
    db.control_finish(&w, &ok.dispatch.unwrap(), json!({"ok":true}), true)
        .await
        .unwrap();
    // All of today: the refund waits for a person, with the local time recorded.
    let now = db
        .guard_rule_save(
            &owner,
            &w,
            &rule(
                "money",
                "time",
                "=",
                json!({"from":"00:00","to":"00:00","days":[today]}),
                "ask",
            ),
        )
        .await
        .unwrap();
    assert!(
        now["summary"]
            .as_str()
            .unwrap()
            .starts_with("Any money action: on ")
    );
    let held = db
        .control_propose(&job, &request(&connection, "refund.create", "T-2", 10.0))
        .await
        .unwrap();
    assert_eq!(held.action["state"], "pending");
    assert_eq!(held.action["guard"]["rule"], now["id"]);
    assert!(held.action["guard"]["facts"]["local_time"].is_string());
    assert_ne!(held.action["guard"]["rule"], elsewhere["id"]);
    // Bad windows are refused.
    for value in [
        json!({"from":"25:00","to":"09:00","days":[]}),
        json!({"from":"21:00","to":"09:00","days":[8]}),
        json!({"from":"21:00","to":"09:00"}),
    ] {
        assert!(
            db.guard_rule_save(&owner, &w, &rule("money", "time", "=", value, "ask"))
                .await
                .is_err()
        );
    }
}

//! The Apps page shows connected services, what agents may do and who uses them;
//! only admins see account names.
use super::tests::{join, user, workspace};
use serde_json::json;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn apps_list_services_with_agent_actions_and_hide_accounts_from_members() {
    let db = super::tests::fixture().await;
    let owner = user(&db, "apps-owner").await;
    let w = workspace(&db, &owner).await;
    let (_, agent) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    let connection = uuid::Uuid::new_v4().to_string();
    let schema = json!({"type":"object","properties":{"order":{"type":"string"}},"required":["order"],"additionalProperties":false});
    let definition = json!({"id":"shop","name":"Shop","account":"orders@shop.test","operations":[
        {"id":"order.read","title":"Find an order","tool":"read_order","effect":"read","schema":schema},
        {"id":"order.delete","title":"Delete an order","tool":"delete_order","effect":"write","schema":schema}]});
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
    super::tool_test_support::grant(&db, &owner, &w, &agent, &connection, "order.read", "allow")
        .await;
    let rule = super::control_actions::guard::GuardRule {
        id: uuid::Uuid::new_v4().to_string(),
        operation: None,
        connection: None,
        meaning: Some("delete".into()),
        source: None,
        kind: "time".into(),
        field: String::new(),
        comparator: "=".into(),
        value: json!({"from":"00:00","to":"00:00","days":[]}),
        effect: "block".into(),
        note: String::new(),
        enabled: true,
        revision: 0,
        trial: false,
    };
    db.guard_rule_save(&owner, &w, &rule).await.unwrap();
    let admin = db.workspace_apps(&owner, &w).await.unwrap();
    assert_eq!(admin["can_manage"], true);
    let app = &admin["apps"][0];
    assert_eq!(
        (
            app["name"].as_str(),
            app["account"].as_str(),
            app["status"].as_str()
        ),
        (Some("Shop"), Some("orders@shop.test"), Some("connected"))
    );
    assert_eq!(app["actions"][0]["mode"], "allow");
    assert_eq!(
        (
            app["actions"][1]["meaning"].as_str(),
            app["actions"][1]["mode"].as_str()
        ),
        (Some("delete"), Some("block"))
    );
    assert_eq!(app["agents"].as_array().unwrap().len(), 1);
    assert_eq!(app["rules"], 1, "the delete rule covers this app");
    let member = user(&db, "apps-member").await;
    join(&db, &owner, &w, &member, "Member").await;
    let seen = db.workspace_apps(&member, &w).await.unwrap();
    assert_eq!(seen["can_manage"], false);
    assert!(
        seen["apps"][0]["account"].is_null(),
        "members don't see account names"
    );
    assert_eq!(seen["apps"][0]["actions"], app["actions"]);
}

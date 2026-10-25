//! Called by the process test with an authenticated HTTP MCP fixture.
use super::tests::*;
use crate::identity::control::{
    broker::{Access, Broker},
    catalog,
    model::Rule,
};
use serde_json::json;

#[tokio::test]
#[ignore = "requires the shared_agent_control.mjs HTTP fixtures and disposable PostgreSQL"]
async fn embedded_gateway_uses_controller_owned_credentials() {
    let path = std::env::var("AYATI_MCP_SERVERS_FILE").unwrap();
    let db = fixture().await;
    let owner = user(&db, "embedded-owner").await;
    let w = workspace(&db, &owner).await;
    let (chat, agent) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    let mut profiles: Vec<catalog::Server> =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(profiles.len(), 1);
    profiles[0].workspace = w.clone();
    std::fs::write(&path, serde_json::to_vec(&profiles).unwrap()).unwrap();
    let server = catalog::server(&w, "orders").unwrap();
    let connection = uuid::Uuid::new_v4().to_string();
    db.control_system(
        &owner,
        &w,
        &crate::identity::control::connections_model::SystemApproval {
            server: "orders".into(),
            approved: true,
            revision: 0,
        },
    )
    .await
    .unwrap();
    db.control_connect(
        &owner,
        &w,
        &agent,
        &connection,
        server.public(),
        &server.fingerprint().unwrap(),
    )
    .await
    .unwrap();
    db.control_rule(
        &owner,
        &w,
        &agent,
        &Rule {
            delegation_revision: None,
            connection: connection.clone(),
            operation: "order.read".into(),
            mode: "allow".into(),
            revision: 0,
            chat: chat.clone(),
        },
    )
    .await
    .unwrap();
    let arguments =
        json!({"connection":connection,"operation":"order.read","arguments":{"order":"one"}});
    db.chat_send(
        &owner,
        &w,
        &chat,
        &message("Read the sample order", Some(&agent)),
    )
    .await
    .unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    let broker = Broker::start(Access::Local(db.clone(), job.clone()))
        .await
        .unwrap();
    let client = reqwest::Client::new();
    let endpoint = format!("http://{}/control", broker.endpoint.address);
    let denied = client
        .post(&endpoint)
        .json(&json!({"token":"wrong","tool":"request_action","arguments":arguments}))
        .send()
        .await
        .unwrap();
    assert_eq!(denied.status(), reqwest::StatusCode::FORBIDDEN);
    assert!(
        db.control_snapshot(&owner, &w, &agent).await.unwrap()["actions"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let response = client
        .post(endpoint)
        .json(&json!({"token":broker.endpoint.token,"tool":"request_action","arguments":arguments}))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
    let snapshot = db.control_snapshot(&owner, &w, &agent).await.unwrap();
    assert_eq!(snapshot["actions"][0]["state"], "completed");
    assert_eq!(snapshot["actions"][0]["operation"], "order.read");
    assert!(
        db.finish_claimed_job(&job, "Read completed", "complete")
            .await
            .unwrap()
    );
}

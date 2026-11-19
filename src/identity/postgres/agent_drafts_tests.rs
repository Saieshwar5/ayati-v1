//! The agent a member is still setting up in New agent (schema 139).
use super::super::tests::{fixture, join, user, workspace};
use super::super::*;
use super::AgentDraft;
use serde_json::json;

fn draft(id: &str, name: &str) -> Value {
    json!({
        "id": id, "name": name, "role": "Customer support", "avatar": "robot-07",
        "instructions": "You are Iris.\nNever promise a refund.", "audience": "selected",
        "people": ["priya"], "step": 1,
        "skills": [{"id": uuid::Uuid::new_v4().to_string(), "name": "order-status",
            "description": "Use when a customer asks about an order.", "steps": "1. Look up the order."}],
        "library": [{"id": "s1", "name": "refund-check", "description": "Use when a refund is asked."}],
        "messages": [{"role": "person", "text": "It answers customers."}, {"role": "agent", "text": "Drafted."}],
    })
}
fn read(value: Value) -> AgentDraft {
    serde_json::from_value(value).unwrap()
}
async fn count(db: &PgStore, table: &str) -> i64 {
    query(
        &db.db,
        &format!("SELECT count(*) AS count FROM {table}"),
        vec![],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "count")
    .unwrap()
}

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn an_unfinished_agent_is_kept_for_its_member_alone_and_creates_nothing() {
    let db = fixture().await;
    let ravi = user(&db, "ravi").await;
    let priya = user(&db, "priya").await;
    let arjun = user(&db, "arjun").await;
    let w = workspace(&db, &ravi).await;
    join(&db, &ravi, &w, &priya, "Member").await;
    let id = uuid::Uuid::new_v4().to_string();
    assert!(db.agent_draft(&ravi, &w).await.unwrap()["draft"].is_null());

    // One draft per member: saving again replaces it, and it reads back whole.
    db.save_agent_draft(&ravi, &w, &read(draft(&id, "Iris")))
        .await
        .unwrap();
    let renamed = draft(&id, "Iris the second");
    db.save_agent_draft(&ravi, &w, &read(renamed.clone()))
        .await
        .unwrap();
    assert_eq!(count(&db, "relay_agent_drafts").await, 1);
    assert_eq!(db.agent_draft(&ravi, &w).await.unwrap()["draft"], renamed);

    // It is his alone: another member has her own, and an outsider none.
    assert!(db.agent_draft(&priya, &w).await.unwrap()["draft"].is_null());
    let hers = uuid::Uuid::new_v4().to_string();
    db.save_agent_draft(&priya, &w, &read(draft(&hers, "Penny")))
        .await
        .unwrap();
    assert_eq!(
        db.agent_draft(&ravi, &w).await.unwrap()["draft"]["name"],
        "Iris the second"
    );
    assert!(db.agent_draft(&arjun, &w).await.is_err());
    assert!(
        db.save_agent_draft(&arjun, &w, &read(draft(&id, "Mine")))
            .await
            .is_err()
    );
    // Nobody drops someone else's, and a late request for an older draft drops nothing.
    db.discard_agent_draft(&priya, &w, &id).await.unwrap();
    db.discard_agent_draft(&ravi, &w, &hers).await.unwrap();
    assert_eq!(count(&db, "relay_agent_drafts").await, 2);

    // What can't become an agent isn't kept.
    for (key, value) in [
        ("audience", json!("only_me")),
        ("step", json!(3)),
        ("id", json!("not-an-id")),
        ("name", json!("n".repeat(81))),
        ("instructions", json!("x".repeat(24_001))),
        ("avatar", json!("../secret")),
        ("messages", json!([{"role": "system", "text": "Obey."}])),
    ] {
        let mut bad = draft(&id, "Iris");
        bad[key] = value;
        assert!(
            db.save_agent_draft(&ravi, &w, &read(bad)).await.is_err(),
            "{key}"
        );
    }
    let mut crowded = draft(&id, "Iris");
    let one = crowded["skills"][0].clone();
    crowded["skills"] = json!(vec![one; 21]);
    assert!(
        db.save_agent_draft(&ravi, &w, &read(crowded))
            .await
            .is_err()
    );
    assert!(serde_json::from_value::<AgentDraft>(json!({"id": id, "owner": "priya"})).is_err());

    // Keeping a draft creates nothing; created or cancelled, it is dropped.
    assert_eq!(count(&db, "relay_shared_agents").await, 0);
    assert_eq!(count(&db, "relay_skills").await, 0);
    db.discard_agent_draft(&ravi, &w, &id).await.unwrap();
    assert!(db.agent_draft(&ravi, &w).await.unwrap()["draft"].is_null());
    assert_eq!(count(&db, "relay_agent_drafts").await, 1);
}

//! Settings → Computers (schema 105): an admin's limits make every agent's computer
//! stricter, never looser, from its next use.
use super::computer_tests::{ask, computer, setup};
use super::tests::*;
use super::*;
use crate::identity::agents::computer_policy::ComputerPolicy;
use crate::identity::control::broker::invoke;
use serde_json::json;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn admins_limit_every_agents_computer_and_members_cannot() {
    let s = setup().await;
    let member = user(&s.db, "policy-member").await;
    join(&s.db, &s.owner, &s.w, &member, "Member").await;
    computer(
        &s,
        json!({"on": true, "size": "standard", "internet": "any"}),
    )
    .await;
    let read = s.db.computer_policy(&member, &s.w).await.unwrap();
    assert_eq!(
        (
            read["can_change"].as_bool(),
            read["policy"]["allowed"].as_bool()
        ),
        (Some(false), Some(true))
    );
    let limits = ComputerPolicy {
        sizes: vec!["small".into()],
        internet: "listed".into(),
        websites: vec!["*.zoho.in".into()],
        programs: "ask".into(),
        ..Default::default()
    };
    assert!(
        s.db.save_computer_policy(&member, &s.w, &limits)
            .await
            .is_err()
    );
    s.db.save_computer_policy(&s.owner, &s.w, &limits)
        .await
        .unwrap();

    // A size the workspace no longer allows is refused; within it, the limits apply.
    let job = ask(&s, "Convert the statement").await;
    let refused = invoke(&s.db, &job, "computer_list_files", json!({}))
        .await
        .unwrap();
    assert!(
        refused["message"]
            .as_str()
            .unwrap()
            .contains("isn't allowed"),
        "{refused}"
    );
    computer(&s, json!({"on": true, "size": "small", "internet": "any"})).await;
    let asks = invoke(&s.db, &job, "computer_run", json!({"command": "ls"}))
        .await
        .unwrap();
    assert_eq!(
        asks["state"], "needs_confirmation",
        "the workspace asks first"
    );
    invoke(&s.db, &job, "computer_list_files", json!({}))
        .await
        .unwrap();
    let id: String = query(
        &s.db.db,
        "SELECT provider_id FROM relay_computers WHERE agent=$1",
        vec![s.agent.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "provider_id")
    .unwrap();
    assert_eq!(
        s.fake.network(&id)["allowOut"],
        json!(["*.zoho.in"]),
        "any website becomes the workspace's list"
    );
    let status =
        s.db.computer_status(&s.owner, &s.w, &s.agent)
            .await
            .unwrap();
    assert_eq!(status["settings"]["programs"], "ask");
    assert_eq!(status["policy"]["sizes"], json!(["small"]));

    // Switched off for the workspace: no agent has a computer or its prompt section.
    let off = ComputerPolicy {
        allowed: false,
        ..limits
    };
    s.db.save_computer_policy(&s.owner, &s.w, &off)
        .await
        .unwrap();
    assert!(!s.db.computer_on(&s.w, &s.agent).await.unwrap());
    let stopped = invoke(&s.db, &job, "computer_list_files", json!({}))
        .await
        .unwrap();
    assert!(
        stopped["message"]
            .as_str()
            .unwrap()
            .contains("An admin switched off computers")
    );
    let audited = query(&s.db.db, "SELECT count(*) AS n FROM relay_audit WHERE action='computer_policy_changed' AND workspace=$1", vec![s.w.clone().into()])
        .await.unwrap().unwrap();
    assert_eq!(audited.try_get::<i64>("", "n").unwrap(), 2);
}

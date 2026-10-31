use super::tests::*;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn released_claims_recover_without_stale_release_or_publication() {
    let db = fixture().await;
    let owner = user(&db, "worker-owner").await;
    let w = workspace(&db, &owner).await;
    let c = chat(&db, &owner, &w, vec![], vec!["agent-one".into()]).await;
    db.chat_send(&owner, &w, &c, &message("recover me", Some("agent-one")))
        .await
        .unwrap();
    let old = db.claim_job().await.unwrap().unwrap();
    db.release_job(&old).await.unwrap();
    let new = db.claim_job().await.unwrap().unwrap();
    assert_ne!(old.claim, new.claim);
    db.release_job(&old).await.unwrap();
    assert!(db.chat_job_allowed(&new).await);
    db.chat_finish(&old, "STALE", "complete").await.unwrap();
    db.chat_finish(&new, "Recovered once", "complete")
        .await
        .unwrap();
    db.chat_finish(&new, "DUPLICATE", "complete").await.unwrap();
    let history = db.chat_messages(&owner, &w, &c).await.unwrap();
    assert_eq!(history["messages"].as_array().unwrap().len(), 2);
    assert_eq!(history["messages"][1]["content"], "Recovered once");
    assert!(db.claim_job().await.unwrap().is_none());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn released_final_attempt_becomes_visible_failure_instead_of_stuck_queue() {
    let db = fixture().await;
    let owner = user(&db, "exhausted-owner").await;
    let w = workspace(&db, &owner).await;
    let c = chat(&db, &owner, &w, vec![], vec!["agent-one".into()]).await;
    db.chat_send(&owner, &w, &c, &message("retry me", Some("agent-one")))
        .await
        .unwrap();
    for _ in 0..3 {
        let job = db.claim_job().await.unwrap().unwrap();
        db.release_job(&job).await.unwrap();
    }
    assert!(db.claim_job().await.unwrap().is_none());
    let history = db.chat_messages(&owner, &w, &c).await.unwrap();
    assert_eq!(history["messages"][1]["state"], "failed");
    assert!(
        history["messages"][1]["content"]
            .as_str()
            .unwrap()
            .contains("recover")
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn replacement_worker_rechecks_revocation_and_profile_scope() {
    let db = fixture().await;
    let owner = user(&db, "scope-owner").await;
    let member = user(&db, "scope-member").await;
    let w = workspace(&db, &owner).await;
    let other = workspace(&db, &member).await;
    join(&db, &owner, &w, &member, "Member").await;
    let agent = uuid::Uuid::new_v4().to_string();
    db.save_shared_agent(&owner, &w, &agent, "Nova", "Reports", "fixture")
        .await
        .unwrap();
    db.chat_grant(&owner, &w, &member.id, &agent, true)
        .await
        .unwrap();
    // A direct conversation: outside a channel, only the grant gives access.
    let c = db
        .chat_create(
            &member,
            &w,
            &crate::identity::chat::model::NewChat {
                id: uuid::Uuid::new_v4().to_string(),
                name: "Nova".into(),
                kind: "direct".into(),
                description: String::new(),
                users: vec![],
                agents: vec![agent.clone()],
            },
        )
        .await
        .unwrap();
    db.chat_send(&member, &w, &c, &message("private request", Some(&agent)))
        .await
        .unwrap();
    let old = db.claim_job().await.unwrap().unwrap();
    db.release_job(&old).await.unwrap();
    db.chat_grant(&owner, &w, &member.id, &agent, false)
        .await
        .unwrap();
    let new = db.claim_job().await.unwrap().unwrap();
    assert!(!db.chat_job_allowed(&new).await);
    let mut forged = new.clone();
    forged.workspace = other.clone();
    assert!(!db.chat_job_allowed(&forged).await);
    assert!(
        db.allowed_shared_agents(&member, &other)
            .await
            .unwrap()
            .is_empty()
    );
    db.chat_finish(&new, "MUST NOT PUBLISH", "complete")
        .await
        .unwrap();
    let history = db.chat_messages(&member, &w, &c).await.unwrap();
    assert_eq!(history["messages"][1]["state"], "cancelled");
    assert!(!history.to_string().contains("MUST NOT PUBLISH"));
}

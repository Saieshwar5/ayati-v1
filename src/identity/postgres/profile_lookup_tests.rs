use super::tests::*;
use crate::identity::agents::config::{SaveAgent, Settings};

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn targeted_profile_lookup_rechecks_grants_and_preserves_private_and_workspace_boundaries() {
    let db = fixture().await;
    let owner = user(&db, "lookup-owner").await;
    let member = user(&db, "lookup-member").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &member, "Member").await;
    let other = workspace(&db, &owner).await;
    let mut agent = SaveAgent {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Scoped lookup".into(),
        role: "Bounded assistance".into(),
        model: "fixture".into(),
        audience: "selected".into(),
        people: None,
        managers: None,
        revision: 0,
        settings: Settings::default(),
    };
    db.save_agent_profile(&owner, &w, &agent).await.unwrap();
    let selected = db
        .allowed_shared_agent(&owner, &w, &agent.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        selected,
        db.allowed_shared_agents(&owner, &w).await.unwrap()[0]
    );
    assert!(
        db.allowed_shared_agent(&owner, &other, &agent.id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        db.allowed_shared_agent(&member, &w, &agent.id)
            .await
            .unwrap()
            .is_none()
    );
    db.chat_grant(&owner, &w, &member.id, &agent.id, true)
        .await
        .unwrap();
    assert_eq!(
        db.allowed_shared_agent(&member, &w, &agent.id)
            .await
            .unwrap(),
        Some(selected)
    );
    db.chat_grant(&owner, &w, &member.id, &agent.id, false)
        .await
        .unwrap();
    assert!(
        db.allowed_shared_agent(&member, &w, &agent.id)
            .await
            .unwrap()
            .is_none()
    );
    agent.id = uuid::Uuid::new_v4().to_string();
    agent.audience = "only_me".into();
    db.save_agent_profile(&member, &w, &agent).await.unwrap();
    assert!(
        db.allowed_shared_agent(&member, &w, &agent.id)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        db.allowed_shared_agent(&owner, &w, &agent.id)
            .await
            .unwrap()
            .is_none()
    );
}

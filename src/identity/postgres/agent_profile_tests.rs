use super::tests::{chat, fixture, join, message, user, workspace};
use super::*;
use crate::identity::agents::config::{SaveAgent, Settings};
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn personal_agents_are_invisible_even_to_workspace_owner_and_cannot_be_shared() {
    let db = fixture().await;
    let owner = user(&db, "a-owner").await;
    let member = user(&db, "a-member").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &member, "User").await;
    let mut a = SaveAgent {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Nova".into(),
        role: "Personal assistant".into(),
        model: "test".into(),
        scope: "private".into(),
        revision: 0,
        settings: Settings::default(),
    };
    db.save_agent_profile(&member, &w, &a).await.unwrap();
    db.save_agent_profile(&member, &w, &a).await.unwrap();
    assert_eq!(db.agent_profiles(&member, &w).await.unwrap().len(), 1);
    assert!(db.agent_profiles(&owner, &w).await.unwrap().is_empty());
    assert!(
        db.allowed_shared_agents(&owner, &w)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(!db.agent_access(&owner.id, &w, &a.id).await.unwrap());
    assert!(db.managed_agent(&owner, &w, &a.id, false).await.is_err());
    assert!(
        db.chat_grant(&owner, &w, &owner.id, &a.id, true)
            .await
            .is_err()
    );
    let mut c = crate::identity::chat::store::NewChat {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Private".into(),
        kind: "channel".into(),
        description: String::new(),
        users: vec![],
        agents: vec![a.id.clone()],
    };
    assert!(db.chat_create(&member, &w, &c).await.is_err());
    c.kind = "direct".into();
    let id = db.chat_create(&member, &w, &c).await.unwrap();
    assert!(db.chat_messages(&owner, &w, &id).await.is_err());
    a.scope = "workspace".into();
    a.revision = 1;
    assert!(db.save_agent_profile(&member, &w, &a).await.is_err());
    a.scope = "private".into();
    a.name = "Changed".into();
    a.revision = 0;
    assert!(db.save_agent_profile(&member, &w, &a).await.is_err());
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn managed_channel_context_and_memory_stay_within_their_audience() {
    let db = fixture().await;
    let owner = user(&db, "context-owner").await;
    let w = workspace(&db, &owner).await;
    let a = SaveAgent {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Mira".into(),
        role: "Purchasing".into(),
        model: "test".into(),
        scope: "workspace".into(),
        revision: 0,
        settings: Settings {
            memory: true,
            ..Default::default()
        },
    };
    db.save_agent_profile(&owner, &w, &a).await.unwrap();
    let c = chat(&db, &owner, &w, vec![], vec![a.id.clone()]).await;
    let other = chat(&db, &owner, &w, vec![], vec![a.id.clone()]).await;
    db.chat_send(&owner, &w, &c, &message("Earlier channel decision", None))
        .await
        .unwrap();
    db.chat_send(&owner, &w, &other, &message("OTHER CHANNEL SECRET", None))
        .await
        .unwrap();
    let job = db
        .chat_send(
            &owner,
            &w,
            &c,
            &message("Use our earlier decision", Some(&a.id)),
        )
        .await
        .unwrap()
        .unwrap();
    assert!(job.prompt.contains("Earlier channel decision"));
    assert!(!job.prompt.contains("OTHER CHANNEL SECRET"));
    let note = KnowledgeInput {
        id: uuid::Uuid::new_v4().to_string(),
        chat: Some(other.clone()),
        title: "Private fact".into(),
        content: "Secret price".into(),
        source_message: None,
        revision: 0,
        forgotten: false,
    };
    db.save_agent_knowledge(&owner, &w, &a.id, &note)
        .await
        .unwrap();
    assert!(
        !db.agent_context_notes(&owner, &w, &a.id, &c)
            .await
            .unwrap()
            .contains("Secret price")
    );
    assert!(
        db.agent_context_notes(&owner, &w, &a.id, &other)
            .await
            .unwrap()
            .contains("Secret price")
    );
    let mut paused = a.clone();
    paused.revision = 1;
    paused.settings.paused = true;
    db.save_agent_profile(&owner, &w, &paused).await.unwrap();
    assert!(
        db.chat_send(&owner, &w, &c, &message("Denied while paused", Some(&a.id)))
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn agent_knowledge_and_avatars_enforce_owner_revision_and_workspace() {
    let db = fixture().await;
    let owner = user(&db, "knowledge-owner").await;
    let member = user(&db, "knowledge-member").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &member, "User").await;
    let a = SaveAgent {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Nova".into(),
        role: "Notes".into(),
        model: "test".into(),
        scope: "private".into(),
        revision: 0,
        settings: Settings::default(),
    };
    db.save_agent_profile(&member, &w, &a).await.unwrap();
    let avatar = db
        .agent_avatar_save(&member, &w, &a.id, "image/png", vec![1, 2, 3])
        .await
        .unwrap();
    assert!(db.agent_avatar(&owner, &w, &a.id, &avatar).await.is_err());
    assert_eq!(
        db.agent_avatar(&member, &w, &a.id, &avatar)
            .await
            .unwrap()
            .1,
        vec![1, 2, 3]
    );
    let mut note = KnowledgeInput {
        id: uuid::Uuid::new_v4().to_string(),
        chat: None,
        title: "Private note".into(),
        content: "Personal information".into(),
        source_message: None,
        revision: 0,
        forgotten: false,
    };
    db.save_agent_knowledge(&member, &w, &a.id, &note)
        .await
        .unwrap();
    assert!(db.agent_knowledge(&owner, &w, &a.id).await.is_err());
    assert!(
        db.save_agent_knowledge(&owner, &w, &a.id, &note)
            .await
            .is_err()
    );
    note.content = "Corrected information".into();
    db.save_agent_knowledge(&member, &w, &a.id, &note)
        .await
        .unwrap();
    assert!(
        db.save_agent_knowledge(&member, &w, &a.id, &note)
            .await
            .is_err()
    );
    note.revision = 1;
    note.forgotten = true;
    db.save_agent_knowledge(&member, &w, &a.id, &note)
        .await
        .unwrap();
    assert!(
        db.agent_knowledge(&member, &w, &a.id)
            .await
            .unwrap()
            .is_empty()
    );
    let other = workspace(&db, &owner).await;
    assert!(
        db.agent_avatar(&owner, &other, &a.id, &avatar)
            .await
            .is_err()
    );
    assert!(
        db.managed_agent(&owner, &other, &a.id, false)
            .await
            .is_err()
    );
    let mut browser = a.clone();
    browser.revision = 1;
    browser.settings.browser = true;
    assert!(db.save_agent_profile(&member, &w, &browser).await.is_err());
}

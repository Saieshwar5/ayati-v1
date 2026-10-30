use super::{
    rehearsal_tests::{setup, start},
    tests::*,
    *,
};
use crate::identity::{
    agents::config::{SaveAgent, Settings},
    library::model::{SavePage, ShareItem},
    rehearsals::model::Source,
};
use serde_json::json;

pub(super) async fn source(
    db: &PgStore,
    owner: &User,
    w: &str,
    agent: &str,
) -> (SavePage, ShareItem, Source) {
    let page = SavePage {
        id: uuid::Uuid::new_v4().to_string(),
        request: uuid::Uuid::new_v4().to_string(),
        revision: 0,
        title: "PRIVATE_DELIVERY_GUIDE".into(),
        document: json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"PRIVATE_SOURCE_FACT: deliveries use the reviewed route."}]}]}),
    };
    db.library_save(owner, w, &page).await.unwrap();
    let sharing = ShareItem {
        request: uuid::Uuid::new_v4().to_string(),
        revision: 1,
        visibility: "private".into(),
        people: vec![],
        agents: vec![agent.into()],
    };
    db.library_share(owner, w, &page.id, &sharing)
        .await
        .unwrap();
    let reference = Source {
        id: page.id.clone(),
        revision: 1,
        access_revision: 2,
    };
    (page, sharing, reference)
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn rehearsal_sources_and_agent_wide_notes_remain_personal_and_revocation_blocks_reads_and_publication()
 {
    let (db, owner, w, chat, agent) = setup().await;
    let (_, mut sharing, reference) = source(&db, &owner, &w, &agent).await;
    let wide = KnowledgeInput {
        id: uuid::Uuid::new_v4().to_string(),
        chat: None,
        title: "Shared guide note".into(),
        content: "APPROVED_AGENT_GUIDANCE".into(),
        source_message: None,
        revision: 0,
        forgotten: false,
    };
    db.save_agent_knowledge(&owner, &w, &agent, &wide)
        .await
        .unwrap();
    let private = KnowledgeInput {
        id: uuid::Uuid::new_v4().to_string(),
        chat: Some(chat),
        title: "Private chat note".into(),
        content: "MUST_NOT_COPY_CONVERSATION_NOTE".into(),
        source_message: None,
        revision: 0,
        forgotten: false,
    };
    db.save_agent_knowledge(&owner, &w, &agent, &private)
        .await
        .unwrap();
    let other = user(&db, "sample-other-owner").await;
    join(&db, &owner, &w, &other, "Member").await;
    db.change(&owner, &w, &other.id, Some("Admin"))
        .await
        .unwrap();
    let options = db.rehearsal_options(&owner, &w, &agent, "").await.unwrap();
    assert_eq!(options["sources"].as_array().unwrap().len(), 1);
    assert!(
        !options
            .to_string()
            .contains("MUST_NOT_COPY_CONVERSATION_NOTE")
    );
    assert!(
        db.rehearsal_options(&other, &w, &agent, "").await.unwrap()["sources"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let mut input = start();
    input.sources = vec![reference.clone()];
    assert!(
        db.start_rehearsal(&other, &w, &agent, &input)
            .await
            .is_err()
    );
    db.start_rehearsal(&owner, &w, &agent, &input)
        .await
        .unwrap();
    assert!(db.rehearsal(&other, &w, &agent, &input.id).await.is_err());
    assert!(
        db.stop_rehearsal(&other, &w, &agent, &input.id)
            .await
            .is_err()
    );
    assert_eq!(
        db.rehearsal_history(&other, &w, &agent, None)
            .await
            .unwrap()["samples"],
        json!([])
    );
    let claim = super::executor_task_tests::claim(&db).await.unwrap();
    assert!(
        claim
            .context
            .reference_notes
            .contains("APPROVED_AGENT_GUIDANCE")
    );
    assert!(
        !claim
            .context
            .reference_notes
            .contains("MUST_NOT_COPY_CONVERSATION_NOTE")
    );
    assert!(
        claim.context.sources[0]["text"]
            .as_str()
            .unwrap()
            .contains("PRIVATE_SOURCE_FACT")
    );
    let cursor = db.sync_page(&owner, &w, None).await.unwrap()["cursor"]
        .as_i64()
        .unwrap();
    sharing.request = uuid::Uuid::new_v4().to_string();
    sharing.revision = 2;
    sharing.agents.clear();
    db.library_share(&owner, &w, &reference.id, &sharing)
        .await
        .unwrap();
    assert_eq!(
        db.sync_page(&owner, &w, Some(cursor)).await.unwrap()["events"],
        json!([{"kind":"sample_access"}])
    );
    assert_eq!(
        db.sync_page(&other, &w, Some(cursor)).await.unwrap()["events"],
        json!([])
    );
    assert!(db.rehearsal(&owner, &w, &agent, &input.id).await.is_err());
    assert_eq!(
        db.rehearsal_history(&owner, &w, &agent, None)
            .await
            .unwrap()["samples"],
        json!([])
    );
    let unavailable = db.rehearsal_options(&owner, &w, &agent, "").await.unwrap();
    assert!(unavailable["active_sample"].is_null());
    assert!(!unavailable.to_string().contains("PRIVATE_DELIVERY_GUIDE"));
    let stopped = db
        .stop_rehearsal(&owner, &w, &agent, &input.id)
        .await
        .unwrap();
    assert_eq!(stopped, json!({"id":input.id,"state":"invalidated"}));
    assert!(!claim.renew().await);
    assert!(!claim.finish("WITHDRAWN_SOURCE_OUTPUT").await);
    sharing.request = uuid::Uuid::new_v4().to_string();
    sharing.revision = 3;
    sharing.agents = vec![agent.clone()];
    db.library_share(&owner, &w, &reference.id, &sharing)
        .await
        .unwrap();
    assert!(
        db.start_rehearsal(&owner, &w, &agent, &input)
            .await
            .is_err()
    );
    assert!(db.rehearsal(&owner, &w, &agent, &input.id).await.is_err());
    assert!(super::executor_task_tests::claim(&db).await.is_none());
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn rehearsal_forgetting_or_correcting_an_approved_note_cannot_publish_or_reveal_the_old_sample()
 {
    let (db, owner, w, _, agent) = setup().await;
    let mut note = KnowledgeInput {
        id: uuid::Uuid::new_v4().to_string(),
        chat: None,
        title: "Guide".into(),
        content: "Old fact".into(),
        source_message: None,
        revision: 0,
        forgotten: false,
    };
    db.save_agent_knowledge(&owner, &w, &agent, &note)
        .await
        .unwrap();
    let input = start();
    db.start_rehearsal(&owner, &w, &agent, &input)
        .await
        .unwrap();
    let claim = super::executor_task_tests::claim(&db).await.unwrap();
    note.content = "Corrected fact".into();
    db.save_agent_knowledge(&owner, &w, &agent, &note)
        .await
        .unwrap();
    assert!(!claim.renew().await);
    assert!(!claim.finish("Old fact must not return").await);
    assert!(db.rehearsal(&owner, &w, &agent, &input.id).await.is_err());
    let next = start();
    db.start_rehearsal(&owner, &w, &agent, &next).await.unwrap();
    let claim = super::executor_task_tests::claim(&db).await.unwrap();
    assert!(claim.context.reference_notes.contains("Corrected fact"));
    assert!(claim.finish("Updated sample").await);
    note.revision = 1;
    note.forgotten = true;
    db.save_agent_knowledge(&owner, &w, &agent, &note)
        .await
        .unwrap();
    assert!(db.rehearsal(&owner, &w, &agent, &next.id).await.is_err());
    assert_eq!(
        db.rehearsal_history(&owner, &w, &agent, None)
            .await
            .unwrap()["samples"],
        json!([])
    );
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn rehearsal_source_version_change_hides_newer_history_without_hiding_older_permitted_samples()
 {
    let (db, owner, w, _, agent) = setup().await;
    let older = start();
    db.start_rehearsal(&owner, &w, &agent, &older)
        .await
        .unwrap();
    let claim = super::executor_task_tests::claim(&db).await.unwrap();
    claim.finish("Ordinary sample").await;
    let (mut source, _, reference) = source(&db, &owner, &w, &agent).await;
    for _ in 0..27 {
        let mut input = start();
        input.sources = vec![reference.clone()];
        db.start_rehearsal(&owner, &w, &agent, &input)
            .await
            .unwrap();
        let claim = super::executor_task_tests::claim(&db).await.unwrap();
        claim.finish("Private sample").await;
    }
    let page = db
        .rehearsal_history(&owner, &w, &agent, None)
        .await
        .unwrap();
    assert_eq!(page["samples"].as_array().unwrap().len(), 25);
    assert!(page["next_before"].is_string());
    assert!(!page.to_string().contains("output"));
    assert!(!page.to_string().contains("PRIVATE_SOURCE_FACT"));
    let next = db
        .rehearsal_history(&owner, &w, &agent, page["next_before"].as_str())
        .await
        .unwrap();
    assert_eq!(next["samples"].as_array().unwrap().len(), 3);
    assert!(
        next["samples"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["id"] == older.id)
    );
    source.revision = 1;
    source.request = uuid::Uuid::new_v4().to_string();
    source.title = "Updated delivery guide".into();
    db.library_save(&owner, &w, &source).await.unwrap();
    let filtered = db
        .rehearsal_history(&owner, &w, &agent, None)
        .await
        .unwrap();
    assert_eq!(filtered["samples"].as_array().unwrap().len(), 1);
    assert_eq!(filtered["samples"][0]["id"], older.id);
    assert!(filtered["next_before"].is_null());
    assert!(
        db.rehearsal_history(&owner, &w, &agent, Some("invalid"))
            .await
            .is_err()
    );
    let other_w = workspace(&db, &owner).await;
    assert!(
        db.rehearsal(&owner, &other_w, &agent, &older.id)
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn rehearsal_private_agent_samples_require_the_owner_and_remain_outside_workspace_owner_access()
 {
    let db = fixture().await;
    let owner = user(&db, "private-sample-workspace-owner").await;
    let member = user(&db, "private-sample-person").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &member, "Member").await;
    let agent = SaveAgent {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Personal helper".into(),
        role: "Prepare private drafts".into(),
        model: "fixture".into(),
        scope: "private".into(),
        revision: 0,
        settings: Settings {
            provider: "openai".into(),
            ..Default::default()
        },
    };
    db.save_agent_profile(&member, &w, &agent).await.unwrap();
    let mut input = start();
    input.agent_revision = 1;
    assert!(
        db.rehearsal_options(&owner, &w, &agent.id, "")
            .await
            .is_err()
    );
    assert!(
        db.start_rehearsal(&owner, &w, &agent.id, &input)
            .await
            .is_err()
    );
    db.start_rehearsal(&member, &w, &agent.id, &input)
        .await
        .unwrap();
    let claim = super::executor_task_tests::claim(&db).await.unwrap();
    assert_eq!(claim.actor, member.id);
    assert_eq!(claim.agent, agent.id);
    assert!(!claim.context.control_tools);
    claim.finish("A private sample response").await;
    assert!(
        db.rehearsal(&owner, &w, &agent.id, &input.id)
            .await
            .is_err()
    );
    assert!(
        db.rehearsal_history(&owner, &w, &agent.id, None)
            .await
            .is_err()
    );
    assert!(
        db.stop_rehearsal(&owner, &w, &agent.id, &input.id)
            .await
            .is_err()
    );
    assert_eq!(
        db.rehearsal(&member, &w, &agent.id, &input.id)
            .await
            .unwrap()["output"],
        format!(
            "{}A private sample response",
            super::rehearsal_queue::SAMPLE_NOTICE
        )
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn rehearsal_membership_changes_retain_exact_receipts_without_reviving_queued_or_running_work()
 {
    let (db, actor, w, _, agent) = setup().await;
    let owner = user(&db, "sample-remaining-owner").await;
    join(&db, &actor, &w, &owner, "Member").await;
    db.change(&actor, &w, &owner.id, Some("Admin"))
        .await
        .unwrap();
    let queued = start();
    db.start_rehearsal(&actor, &w, &agent, &queued)
        .await
        .unwrap();
    db.change(&owner, &w, &actor.id, Some("Member"))
        .await
        .unwrap();
    // The agent creator remains its maintainer after demotion, but work
    // accepted under an earlier workspace role is not allowed to continue.
    assert_eq!(
        db.rehearsal(&actor, &w, &agent, &queued.id).await.unwrap()["state"],
        "invalidated"
    );
    db.change(&owner, &w, &actor.id, Some("Admin"))
        .await
        .unwrap();
    assert_eq!(
        db.start_rehearsal(&actor, &w, &agent, &queued)
            .await
            .unwrap()["state"],
        "invalidated"
    );
    assert!(super::executor_task_tests::claim(&db).await.is_none());
    let running = start();
    db.start_rehearsal(&actor, &w, &agent, &running)
        .await
        .unwrap();
    let claim = super::executor_task_tests::claim(&db).await.unwrap();
    db.change(&owner, &w, &actor.id, None).await.unwrap();
    assert!(
        db.start_rehearsal(&actor, &w, &agent, &running)
            .await
            .is_err()
    );
    join(&db, &owner, &w, &actor, "Member").await;
    db.change(&owner, &w, &actor.id, Some("Admin"))
        .await
        .unwrap();
    assert_eq!(
        db.start_rehearsal(&actor, &w, &agent, &running)
            .await
            .unwrap()["state"],
        "invalidated"
    );
    assert!(!claim.renew().await);
    assert!(!claim.finish("LATE_OUTPUT").await);
    assert!(super::executor_task_tests::claim(&db).await.is_none());
    assert_eq!(
        db.rehearsal_history(&actor, &w, &agent, None)
            .await
            .unwrap()["samples"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

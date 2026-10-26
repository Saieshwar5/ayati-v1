use super::{control_tests::setup, tests::*, *};
use crate::{
    identity::library::model::{AskItem, SavePage, ShareItem},
    platform::panels::PanelDraft,
};
use serde_json::json;
fn draft() -> PanelDraft {
    serde_json::from_value(json!({"id":uuid::Uuid::new_v4().to_string(),"expected_revision":0,"title":"PRIVATE_SOURCE_TITLE","purpose":"","sources":[],"sections":[{"kind":"metric","label":"Source amount","value":"PRIVATE_SOURCE_VALUE"}]})).unwrap()
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn home_library_provenance_guards_active_drafts_tools_and_review_after_revocation_and_regrant()
 {
    let (db, owner, w, _, initial) = setup("allow").await;
    db.chat_finish(&initial, "Done", "complete").await.unwrap();
    let page = SavePage {
        id: uuid::Uuid::new_v4().to_string(),
        request: uuid::Uuid::new_v4().to_string(),
        revision: 0,
        title: "Panel source".into(),
        document: json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"PRIVATE_SOURCE_AMOUNT"}]}]}),
    };
    db.library_save(&owner, &w, &page).await.unwrap();
    let mut sharing = ShareItem {
        request: uuid::Uuid::new_v4().to_string(),
        revision: 1,
        visibility: "private".into(),
        people: vec![],
        agents: vec![initial.agent.clone()],
    };
    db.library_share(&owner, &w, &page.id, &sharing)
        .await
        .unwrap();
    db.library_ask(
        &owner,
        &w,
        &page.id,
        &AskItem {
            request: uuid::Uuid::new_v4().to_string(),
            revision: 1,
            agent: initial.agent.clone(),
            question: "Prepare a personal metric".into(),
        },
    )
    .await
    .unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    let mut input = draft();
    db.save_personal_panel(&job, &input).await.unwrap();
    db.review_home_panel(
        &owner,
        &w,
        &input.id,
        &crate::identity::home_panels::model::Review {
            request: uuid::Uuid::new_v4().to_string(),
            revision: 1,
        },
    )
    .await
    .unwrap();
    input.expected_revision = 1;
    input.purpose = "Updated presentation".into();
    db.save_personal_panel(&job, &input).await.unwrap();
    sharing.request = uuid::Uuid::new_v4().to_string();
    sharing.revision = 2;
    sharing.agents.clear();
    db.library_share(&owner, &w, &page.id, &sharing)
        .await
        .unwrap();
    let current = db.home_panels(&owner, &w).await.unwrap();
    assert!(!current.to_string().contains("PRIVATE_SOURCE"));
    assert_eq!(current["panels"][0]["active"]["available"], false);
    assert_eq!(current["panels"][0]["draft"]["available"], false);
    assert!(
        db.review_home_panel(
            &owner,
            &w,
            &input.id,
            &crate::identity::home_panels::model::Review {
                request: uuid::Uuid::new_v4().to_string(),
                revision: 2
            }
        )
        .await
        .is_err()
    );
    sharing.request = uuid::Uuid::new_v4().to_string();
    sharing.revision = 3;
    sharing.agents.push(initial.agent.clone());
    db.library_share(&owner, &w, &page.id, &sharing)
        .await
        .unwrap();
    assert_eq!(
        db.home_panels(&owner, &w).await.unwrap()["panels"][0]["active"]["available"],
        false
    );
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn home_source_conversation_visibility_is_independent_of_owner_role_and_personal_invalidations_disclose_no_private_ids()
 {
    let (db, owner, w, conn, job) = setup("allow").await;
    let input = draft();
    db.save_personal_panel(&job, &input).await.unwrap();
    let other = user(&db, "home-other-member").await;
    join(&db, &owner, &w, &other, "User").await;
    db.change(&owner, &w, &other.id, Some("Owner"))
        .await
        .unwrap();
    let cursor = db.sync_page(&other, &w, None).await.unwrap()["cursor"]
        .as_i64()
        .unwrap();
    let keep = crate::identity::home_panels::model::Review {
        request: uuid::Uuid::new_v4().to_string(),
        revision: 1,
    };
    db.review_home_panel(&owner, &w, &input.id, &keep)
        .await
        .unwrap();
    let events = db.sync_page(&other, &w, Some(cursor)).await.unwrap();
    assert!(!events.to_string().contains(&input.id));
    assert!(
        !events["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["kind"] == "panel_access")
    );
    execute(
        &db.db,
        "DELETE FROM relay_chat_people WHERE workspace=$1 AND chat=$2 AND user_id=$3",
        vec![
            w.clone().into(),
            job.chat.clone().into(),
            owner.id.clone().into(),
        ],
    )
    .await
    .unwrap();
    assert!(
        !db.home_panels(&owner, &w)
            .await
            .unwrap()
            .to_string()
            .contains("PRIVATE_SOURCE")
    );
    assert!(
        db.review_home_panel(&other, &w, &input.id, &keep)
            .await
            .is_err()
    );
    let _ = conn;
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn panel_new_work_cannot_bypass_question_waits_current_worker_ownership_or_original_person() {
    let (db, owner, w, _, job) = setup("ask").await;
    let input = draft();
    let mut forged = job.clone();
    forged.requester = uuid::Uuid::new_v4().to_string();
    assert!(db.save_personal_panel(&forged, &input).await.is_err());
    let mut stale = job.clone();
    stale.claim = Some(uuid::Uuid::new_v4().to_string());
    assert!(db.save_personal_panel(&stale, &input).await.is_err());
    db.ask_question(&job, &super::attention_tests::ask())
        .await
        .unwrap();
    assert!(db.save_personal_panel(&job, &input).await.is_err());
    db.cancel_assignment(&owner, &w, &job.chat, &job.reply)
        .await
        .unwrap();
    assert!(db.personal_panels(&job).await.is_err());
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn panel_collection_revision_template_and_layout_bounds_reject_without_partial_records() {
    let (db, owner, w, _, job) = setup("allow").await;
    let mut input = draft();
    input.sections =
        serde_json::from_value(json!([{"kind":"table","columns":[],"rows":[]}])).unwrap();
    assert!(db.save_personal_panel(&job, &input).await.is_err());
    let mut input = draft();
    for i in 0..32 {
        input.id = uuid::Uuid::new_v4().to_string();
        input.title = format!("Personal panel {i}");
        db.save_personal_panel(&job, &input).await.unwrap();
    }
    input.id = uuid::Uuid::new_v4().to_string();
    input.title = "One too many".into();
    assert!(db.save_personal_panel(&job, &input).await.is_err());
    assert_eq!(
        db.home_panels(&owner, &w).await.unwrap()["panels"]
            .as_array()
            .unwrap()
            .len(),
        32
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn panel_agent_discovery_returns_no_saved_titles_and_another_conversation_cannot_discover_or_revise_it()
 {
    let (db, owner, w, _, job) = setup("allow").await;
    let mut input = draft();
    db.save_personal_panel(&job, &input).await.unwrap();
    let identities = db.personal_panels(&job).await.unwrap();
    assert_eq!(identities["panels"][0]["id"], input.id);
    assert!(!identities.to_string().contains("PRIVATE_SOURCE"));
    assert!(identities["panels"][0].get("title").is_none());
    db.chat_finish(&job, "A personal draft is saved", "complete")
        .await
        .unwrap();
    let other_chat = chat(&db, &owner, &w, vec![], vec![job.agent.clone()]).await;
    db.chat_send(
        &owner,
        &w,
        &other_chat,
        &message("A different conversation request", Some(&job.agent)),
    )
    .await
    .unwrap();
    let other = db.claim_job().await.unwrap().unwrap();
    assert_eq!(
        db.personal_panels(&other).await.unwrap()["panels"],
        json!([])
    );
    input.expected_revision = 1;
    input.purpose = "A change from a broader conversation".into();
    assert!(db.save_personal_panel(&other, &input).await.is_err());
    assert!(
        execute(
            &db.db,
            "UPDATE relay_home_panels SET chat=$3 WHERE workspace=$1 AND id=$2",
            vec![w.clone().into(), input.id.clone().into(), other_chat.into()]
        )
        .await
        .is_err()
    );
    assert_eq!(
        db.home_panels(&owner, &w).await.unwrap()["panels"][0]["revision"],
        1
    );
}

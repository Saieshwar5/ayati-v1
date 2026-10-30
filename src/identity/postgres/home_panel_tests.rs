use super::{
    control_tests::{request, setup},
    tests::*,
    *,
};
use crate::{
    identity::home_panels::model::{Layout, Review},
    panels::PanelDraft,
};
use serde_json::json;
fn draft() -> PanelDraft {
    serde_json::from_value(json!({"id":uuid::Uuid::new_v4().to_string(),"expected_revision":0,"title":"PRIVATE_PANEL_TITLE","purpose":"Review follow-up","template":"brief","sections":[{"kind":"text","heading":"Follow-up","body":"PRIVATE_PANEL_BODY"}],"sources":[]})).unwrap()
}
fn review(revision: i64) -> Review {
    Review {
        request: uuid::Uuid::new_v4().to_string(),
        revision,
    }
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn personal_panels_save_immutable_drafts_review_exact_revisions_and_sync_only_the_original_persons_placement()
 {
    let (db, owner, w, _, job) = setup("allow").await;
    let input = draft();
    let a = db.save_personal_panel(&job, &input).await.unwrap();
    let b = db.save_personal_panel(&job, &input).await.unwrap();
    assert_eq!(a["id"], b["id"]);
    let mut semantic: PanelDraft =
        serde_json::from_value(serde_json::to_value(&input).unwrap()).unwrap();
    semantic.id = uuid::Uuid::new_v4().to_string();
    assert_eq!(
        db.save_personal_panel(&job, &semantic).await.unwrap()["id"],
        input.id
    );
    let list = db.home_panels(&owner, &w).await.unwrap();
    assert_eq!(list["panels"][0]["active"], Value::Null);
    assert_eq!(
        list["panels"][0]["draft"]["definition"]["title"],
        input.title
    );
    let viewer = user(&db, "panel-other-owner").await;
    join(&db, &owner, &w, &viewer, "Member").await;
    db.change(&owner, &w, &viewer.id, Some("Admin"))
        .await
        .unwrap();
    assert_eq!(
        db.home_panels(&viewer, &w).await.unwrap()["panels"],
        json!([])
    );
    assert!(
        db.review_home_panel(&viewer, &w, &input.id, &review(1))
            .await
            .is_err()
    );
    let keep = review(1);
    let saved = db
        .review_home_panel(&owner, &w, &input.id, &keep)
        .await
        .unwrap();
    assert_eq!(
        db.review_home_panel(&owner, &w, &input.id, &keep)
            .await
            .unwrap(),
        saved
    );
    let mut changed = keep;
    changed.revision = 2;
    assert!(
        db.review_home_panel(&owner, &w, &input.id, &changed)
            .await
            .is_err()
    );
    let first = Layout {
        request: uuid::Uuid::new_v4().to_string(),
        revision: 0,
        order: vec![input.id.clone()],
        wide: vec![input.id.clone()],
        hidden: vec![],
    };
    let layout = db.save_home_layout(&owner, &w, &first).await.unwrap();
    assert_eq!(layout["revision"], 1);
    assert_eq!(
        db.save_home_layout(&owner, &w, &first).await.unwrap(),
        layout
    );
    let mut stale = first.clone();
    stale.request = uuid::Uuid::new_v4().to_string();
    assert!(db.save_home_layout(&owner, &w, &stale).await.is_err());
    assert_eq!(
        db.home_panels(&viewer, &w).await.unwrap()["layout"]["revision"],
        0
    );
    let mut revision: PanelDraft =
        serde_json::from_value(serde_json::to_value(&input).unwrap()).unwrap();
    revision.expected_revision = 1;
    revision.sections = serde_json::from_value(
        json!([{"kind":"text","heading":"Changed","body":"NEW_PANEL_BODY"}]),
    )
    .unwrap();
    db.save_personal_panel(&job, &revision).await.unwrap();
    let current = db.home_panels(&owner, &w).await.unwrap();
    assert_eq!(
        current["panels"][0]["active"]["definition"]["sections"][0]["body"],
        "PRIVATE_PANEL_BODY"
    );
    assert_eq!(
        current["panels"][0]["draft"]["definition"]["sections"][0]["body"],
        "NEW_PANEL_BODY"
    );
    assert!(
        db.review_home_panel(&owner, &w, &input.id, &review(1))
            .await
            .is_err()
    );
    assert!(
        execute(
            &db.db,
            "UPDATE relay_home_panel_versions SET definition='{}' WHERE workspace=$1",
            vec![w.clone().into()]
        )
        .await
        .is_err()
    );
    db.review_home_panel(&owner, &w, &input.id, &review(2))
        .await
        .unwrap();
    assert_eq!(
        db.home_panels(&owner, &w).await.unwrap()["panels"][0]["active_revision"],
        2
    );
    db.chat_finish(&job, "Panel prepared", "complete")
        .await
        .unwrap();
    assert!(db.home_panels(&owner, &w).await.unwrap()["panels"][0]["active"]["available"] == true);
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn panel_operation_provenance_is_automatic_retained_on_revisions_and_regrant_cannot_restore_old_data()
 {
    let (db, owner, w, conn, job) = setup("allow").await;
    let action = db
        .control_propose(&job, &request(&conn, "panel-receipt"))
        .await
        .unwrap();
    db.control_finish(&w, &action.dispatch.unwrap(), json!({"ok":true}), true)
        .await
        .unwrap();
    let input = draft();
    db.save_personal_panel(&job, &input).await.unwrap();
    db.review_home_panel(&owner, &w, &input.id, &review(1))
        .await
        .unwrap();
    let data = db.home_panels(&owner, &w).await.unwrap();
    assert_eq!(data["panels"][0]["active"]["source_operations"], 1);
    let cursor = db.sync_page(&owner, &w, None).await.unwrap()["cursor"]
        .as_i64()
        .unwrap();
    let mut rule = crate::identity::control::model::Rule {
        connection: conn.clone(),
        operation: "order.update".into(),
        mode: "block".into(),
        revision: 1,
        chat: job.chat.clone(),
        approver: None,
        approver_revision: None,
        approval_chat: None,
        delegation_revision: None,
    };
    db.control_rule(&owner, &w, &job.agent, &rule)
        .await
        .unwrap();
    assert_eq!(
        db.home_panels(&owner, &w).await.unwrap()["panels"][0]["active"]["available"],
        false
    );
    assert!(
        !db.home_panels(&owner, &w)
            .await
            .unwrap()
            .to_string()
            .contains("PRIVATE_PANEL")
    );
    assert!(
        db.sync_page(&owner, &w, Some(cursor)).await.unwrap()["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["kind"] == "panel_access")
    );
    rule.mode = "allow".into();
    rule.revision = 2;
    db.control_rule(&owner, &w, &job.agent, &rule)
        .await
        .unwrap();
    assert_eq!(
        db.home_panels(&owner, &w).await.unwrap()["panels"][0]["active"]["available"],
        false
    );
    let mut update = input;
    update.expected_revision = 1;
    update.title = "A changed title cannot discard old sources".into();
    assert!(db.save_personal_panel(&job, &update).await.is_err());
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn panel_authority_limits_and_receipts_cannot_be_forged_or_replayed_after_stop() {
    let (db, owner, w, _, job) = setup("ask").await;
    let mut input = draft();
    input.publish = true;
    assert!(db.save_personal_panel(&job, &input).await.is_err());
    input.publish = false;
    input.sources = vec![uuid::Uuid::new_v4().to_string()];
    assert!(db.save_personal_panel(&job, &input).await.is_err());
    input.sources.clear();
    db.save_personal_panel(&job, &input).await.unwrap();
    let wrong = workspace(&db, &owner).await;
    assert!(
        db.review_home_panel(&owner, &wrong, &input.id, &review(1))
            .await
            .is_err()
    );
    let foreign = Layout {
        request: uuid::Uuid::new_v4().to_string(),
        revision: 0,
        order: vec![uuid::Uuid::new_v4().to_string()],
        wide: vec![],
        hidden: vec![],
    };
    assert!(db.save_home_layout(&owner, &w, &foreign).await.is_err());
    db.cancel_assignment(&owner, &w, &job.chat, &job.reply)
        .await
        .unwrap();
    assert!(db.save_personal_panel(&job, &input).await.is_err());
}

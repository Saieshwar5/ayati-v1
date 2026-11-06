//! Library LB3: agents suggest, edit and save pages only as far as their level, the
//! person they work for and "use for" allow; every change is a version marked with
//! the agent, and suggestions wait for someone who can edit.
use super::super::{tests::*, *};
use super::folders::{ask, grant, setup, share};
use crate::identity::control::broker::invoke;
use crate::identity::library::model::*;
use crate::identity::postgres::Decision;
use serde_json::json;

async fn policies(s: &super::folders::Setup) -> (String, i64) {
    let folder = s.db.library_get(&s.owner, &s.w, &s.returns).await.unwrap()["folder"]
        .as_str()
        .unwrap()
        .to_owned();
    let revision = s.db.library_get(&s.owner, &s.w, &folder).await.unwrap()["access_revision"]
        .as_i64()
        .unwrap();
    (folder, revision)
}
async fn level(s: &super::folders::Setup, level: &str) -> String {
    let (folder, revision) = policies(s).await;
    let mut g = grant(&s.agent, "staff");
    g.level = level.into();
    s.db.library_share(
        &s.owner,
        &s.w,
        &folder,
        &share(revision, "private", vec![], vec![g]),
    )
    .await
    .unwrap();
    folder
}
fn decide(action: &str) -> Decision {
    Decision {
        request: uuid::Uuid::new_v4().to_string(),
        action: action.into(),
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn suggestions_wait_for_someone_who_can_edit_the_page() {
    let s = setup().await;
    let db = &s.db;
    level(&s, "suggest").await;
    // A colleague who can't open the policy can't have the agent change it.
    let theirs = chat(db, &s.member, &s.w, vec![], vec![s.agent.clone()]).await;
    let job = ask(&s, &s.member, &theirs, "Make it 45 days").await;
    assert_eq!(
        invoke(
            db,
            &job,
            "suggest_edit",
            json!({"id":s.returns,"text":"45 days"})
        )
        .await
        .unwrap()["state"],
        "not_allowed"
    );
    db.chat_finish(&job, "No.", "complete").await.unwrap();
    let private = chat(db, &s.owner, &s.w, vec![], vec![s.agent.clone()]).await;
    let mine = ask(&s, &s.owner, &private, "Make it 45 days").await;
    assert_eq!(
        invoke(
            db,
            &mine,
            "edit_document",
            json!({"id":s.returns,"revision":1,"text":"x"})
        )
        .await
        .unwrap()["state"],
        "not_allowed",
        "suggest level can't edit"
    );
    let first = invoke(db, &mine, "suggest_edit", json!({"id":s.returns,"text":"# Returns\nThe refund window is 45 days.","note":"Ravi asked"})).await.unwrap();
    assert_eq!(first["state"], "suggested");
    let second = invoke(
        db,
        &mine,
        "suggest_edit",
        json!({"id":s.returns,"text":"Refunds within 60 days."}),
    )
    .await
    .unwrap();
    db.chat_finish(&mine, "Suggested.", "complete")
        .await
        .unwrap();
    assert!(
        db.library_suggestions(&s.member, &s.w, &s.returns)
            .await
            .is_err()
    );
    let waiting = db
        .library_suggestions(&s.owner, &s.w, &s.returns)
        .await
        .unwrap();
    assert_eq!(waiting["suggestions"].as_array().unwrap().len(), 2);
    // Suggestions made in the same second may list in either order.
    let mine = waiting["suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["id"] == first["suggestion"])
        .unwrap();
    assert_eq!(
        (mine["agent"].as_str(), mine["note"].as_str()),
        (Some("Pool agent"), Some("Ravi asked"))
    );
    assert_eq!(
        db.library_get(&s.owner, &s.w, &s.returns).await.unwrap()["revision"],
        1,
        "nothing changes until accepted"
    );
    let id = |v: &Value| v["suggestion"].as_str().unwrap().to_owned();
    db.library_decide_suggestion(&s.owner, &s.w, &s.returns, &id(&second), &decide("reject"))
        .await
        .unwrap();
    let accepted = db
        .library_decide_suggestion(&s.owner, &s.w, &s.returns, &id(&first), &decide("accept"))
        .await
        .unwrap();
    assert_eq!(accepted["revision"], 2);
    let page = db.library_get(&s.owner, &s.w, &s.returns).await.unwrap();
    assert_eq!(page["document"]["content"][0]["type"], "heading");
    let history = db
        .library_versions(&s.owner, &s.w, &s.returns)
        .await
        .unwrap();
    assert_eq!(history["versions"][0]["agent"], "Pool agent");
    assert!(
        db.library_decide_suggestion(&s.owner, &s.w, &s.returns, &id(&first), &decide("accept"))
            .await
            .is_err(),
        "decided once"
    );
    // A suggestion on an older revision can only be rejected.
    let again = ask(&s, &s.owner, &private, "Once more").await;
    let late = invoke(
        db,
        &again,
        "suggest_edit",
        json!({"id":s.returns,"text":"90 days"}),
    )
    .await
    .unwrap();
    db.chat_finish(&again, "Suggested.", "complete")
        .await
        .unwrap();
    db.library_save(&s.owner, &s.w, &SavePage { id: s.returns.clone(), request: uuid::Uuid::new_v4().to_string(), revision: 2, title: "Returns policy".into(), document: json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Owner's own edit"}]}]}), folder: None }).await.unwrap();
    assert!(
        db.library_suggestions(&s.owner, &s.w, &s.returns)
            .await
            .unwrap()["suggestions"][0]["stale"]
            == true
    );
    assert!(
        db.library_decide_suggestion(&s.owner, &s.w, &s.returns, &id(&late), &decide("accept"))
            .await
            .is_err()
    );
    db.library_decide_suggestion(&s.owner, &s.w, &s.returns, &id(&late), &decide("reject"))
        .await
        .unwrap();
    let audit: Vec<String> = rows(&db.db, "SELECT summary FROM relay_audit WHERE workspace=$1 AND action LIKE 'library_suggestion%' ORDER BY at,summary", vec![s.w.clone().into()])
        .await.unwrap().into_iter().map(|r| r.try_get("", "summary").unwrap()).collect();
    assert!(
        audit.contains(
            &"Accepted Pool agent's change to \"Returns policy\" (revision 2)".to_owned()
        )
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn agents_edit_and_save_as_versions_within_their_level_and_audience() {
    let s = setup().await;
    let db = &s.db;
    let folder = level(&s, "edit").await;
    let private = chat(db, &s.owner, &s.w, vec![], vec![s.agent.clone()]).await;
    let job = ask(&s, &s.owner, &private, "Update the refund window").await;
    assert_eq!(
        invoke(
            db,
            &job,
            "edit_document",
            json!({"id":s.returns,"revision":7,"text":"x"})
        )
        .await
        .unwrap()["state"],
        "changed"
    );
    let edited = invoke(
        db,
        &job,
        "edit_document",
        json!({"id":s.returns,"revision":1,"text":"The refund window is 45 days."}),
    )
    .await
    .unwrap();
    assert_eq!(
        (edited["state"].as_str(), edited["revision"].as_i64()),
        (Some("edited"), Some(2))
    );
    assert_eq!(
        invoke(
            db,
            &job,
            "save_document",
            json!({"folder":folder,"title":"Quote","text":"x"})
        )
        .await
        .unwrap()["state"],
        "not_allowed",
        "edit level can't save new pages"
    );
    let found = invoke(db, &job, "search_library", json!({"query":"45 days"}))
        .await
        .unwrap();
    assert_eq!(
        found["results"][0]["revision"], 2,
        "an agent's edit is searchable at once"
    );
    db.chat_finish(&job, "Done.", "complete").await.unwrap();
    // Undo is restoring the earlier version.
    db.library_action(
        &s.owner,
        &s.w,
        &s.returns,
        &ItemAction {
            request: uuid::Uuid::new_v4().to_string(),
            revision: 2,
            action: "restore_version".into(),
            version: Some(1),
        },
    )
    .await
    .unwrap();
    assert!(
        db.library_get(&s.owner, &s.w, &s.returns).await.unwrap()["document"]
            .to_string()
            .contains("30 days")
    );
    // Save here files a new page in the folder, as the person, marked with the agent.
    let folder = level(&s, "save").await;
    let next = ask(&s, &s.owner, &private, "Make a quote").await;
    let saved = invoke(
        db,
        &next,
        "save_document",
        json!({"folder":folder,"title":"Quote for Acme","text":"# Quote\n- Desk: 1250"}),
    )
    .await
    .unwrap();
    assert_eq!(saved["state"], "saved");
    let page = db
        .library_get(&s.owner, &s.w, saved["id"].as_str().unwrap())
        .await
        .unwrap();
    assert_eq!(
        (page["folder"].as_str(), page["created_by"].as_str()),
        (Some(folder.as_str()), Some(s.owner.id.as_str()))
    );
    assert_eq!(
        db.library_versions(&s.owner, &s.w, saved["id"].as_str().unwrap())
            .await
            .unwrap()["versions"][0]["agent"],
        "Pool agent"
    );
    db.chat_finish(&next, "Saved.", "complete").await.unwrap();
    // Not where a colleague who can't see HR will read the answer, and not on an item.
    let mut hr = grant(&s.agent, "viewers");
    hr.level = "edit".into();
    db.library_share(
        &s.owner,
        &s.w,
        &s.hr,
        &share(2, "private", vec![], vec![hr.clone()]),
    )
    .await
    .unwrap();
    let shared = chat(
        db,
        &s.owner,
        &s.w,
        vec![s.member.id.clone()],
        vec![s.agent.clone()],
    )
    .await;
    let group = ask(&s, &s.owner, &shared, "Raise the band").await;
    assert_eq!(
        invoke(
            db,
            &group,
            "edit_document",
            json!({"id":s.salaries,"revision":1,"text":"50k"})
        )
        .await
        .unwrap()["state"],
        "not_found"
    );
    db.chat_finish(&group, "No.", "complete").await.unwrap();
    let mut on_item = grant(&s.agent, "staff");
    on_item.level = "save".into();
    let fresh = super::folders::page("Loose page", "Text", None);
    db.library_save(&s.owner, &s.w, &fresh).await.unwrap();
    assert!(
        db.library_share(
            &s.owner,
            &s.w,
            &fresh.id,
            &share(1, "private", vec![], vec![on_item])
        )
        .await
        .is_err()
    );
    let mut reserved = grant(&s.agent, "staff");
    reserved.level = "edit_approval".into();
    assert!(
        db.library_share(
            &s.owner,
            &s.w,
            &fresh.id,
            &share(1, "private", vec![], vec![reserved])
        )
        .await
        .is_err()
    );
}

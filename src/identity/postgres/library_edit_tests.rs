//! Suggestions: a page change from an agent that may only suggest waits for someone who
//! can edit. Editing and saving new files are covered by the workbench tests.
use super::super::{tests::*, *};
use super::folders::{ask, grant, setup, share};
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
/// What saving a page from the agent's computer does: a suggestion that waits, or the
/// next version. (The agent reaches these through open_document and save_file.)
async fn page_tool(
    db: &PgStore,
    job: &crate::identity::chat::model::Job,
    kind: &str,
    input: Value,
) -> Result<Value> {
    let document = crate::identity::library::page_markdown::from_markdown(
        input["text"].as_str().unwrap_or(""),
    );
    let plain = crate::identity::library::model::document(&document)?;
    let id = input["id"].as_str().unwrap_or("");
    if kind == "suggest_edit" {
        db.library_suggest_page(
            job,
            id,
            input["note"].as_str().unwrap_or(""),
            document,
            plain,
        )
        .await
    } else {
        db.library_edit_page(job, id, input["revision"].as_i64(), document, plain)
            .await
    }
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
        page_tool(
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
        page_tool(
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
    let first = page_tool(db, &mine, "suggest_edit", json!({"id":s.returns,"text":"# Returns\nThe refund window is 45 days.","note":"Ravi asked"})).await.unwrap();
    assert_eq!(first["state"], "suggested");
    let second = page_tool(
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
    let late = page_tool(
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

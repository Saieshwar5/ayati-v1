//! Schema 71: a Library edit or an unrelated sharing change keeps earlier answers;
//! changing how a document is shared removes only the answers that used it, from
//! Chat and from agents' conversation history.
use super::super::{tests::*, *};
use super::folders::{ask, grant, setup, share};
use crate::identity::control::broker::invoke;
use crate::identity::library::model::*;
use serde_json::json;

async fn epochs(db: &PgStore, w: &str) -> i64 {
    query(&db.db, "SELECT COALESCE(sum(revision),0)::bigint AS n FROM relay_context_epochs WHERE workspace=$1", vec![w.into()])
        .await
        .unwrap()
        .unwrap()
        .try_get("", "n")
        .unwrap()
}
fn shown(listing: &Value, id: &str) -> String {
    listing["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == id)
        .map(|m| m["content"].as_str().unwrap_or("").to_owned())
        .unwrap_or_default()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn sharing_a_document_differently_removes_only_the_answers_that_used_it() {
    let s = setup().await;
    let db = &s.db;
    let theirs = chat(db, &s.member, &s.w, vec![], vec![s.agent.clone()]).await;
    let used = ask(&s, &s.member, &theirs, "What is our refund window?").await;
    invoke(db, &used, "read_document", json!({"id":s.returns}))
        .await
        .unwrap();
    db.chat_finish(&used, "REFUND_ANSWER 30 days.", "complete")
        .await
        .unwrap();
    let plain = ask(&s, &s.member, &theirs, "Thanks!").await;
    db.chat_finish(&plain, "PLAIN_ANSWER You're welcome.", "complete")
        .await
        .unwrap();
    // Editing the policy, moving an unrelated page and sharing HR change no epoch.
    let before = epochs(db, &s.w).await;
    let current = db.library_get(&s.owner, &s.w, &s.returns).await.unwrap();
    db.library_save(&s.owner, &s.w, &SavePage {
        id: s.returns.clone(),
        request: uuid::Uuid::new_v4().to_string(),
        revision: current["revision"].as_i64().unwrap(),
        title: "Returns policy".into(),
        document: json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"The refund window is 45 days."}]}]}),
        folder: None,
    })
    .await
    .unwrap();
    db.library_share(
        &s.owner,
        &s.w,
        &s.hr,
        &share(2, "private", vec![], vec![grant(&s.agent, "staff")]),
    )
    .await
    .unwrap();
    assert_eq!(
        epochs(db, &s.w).await,
        before,
        "Library changes no longer fence the workspace"
    );
    let view = db.chat_messages(&s.member, &s.w, &theirs).await.unwrap();
    assert!(
        shown(&view, &used.reply).starts_with("REFUND_ANSWER"),
        "an edit keeps the answer"
    );
    assert!(shown(&view, &plain.reply).starts_with("PLAIN_ANSWER"));
    let next = ask(&s, &s.member, &theirs, "What did you say earlier?").await;
    let history = invoke(
        db,
        &next,
        "search_conversation_history",
        json!({"query":"ANSWER"}),
    )
    .await
    .unwrap();
    assert_eq!(history["messages"].as_array().unwrap().len(), 2);
    db.chat_finish(&next, "NEXT_ANSWER", "complete")
        .await
        .unwrap();
    // Sharing the folder the policy is in differently removes the answer that read it.
    let folder = db.library_get(&s.owner, &s.w, &s.returns).await.unwrap()["folder"]
        .as_str()
        .unwrap()
        .to_owned();
    let revision = db.library_get(&s.owner, &s.w, &folder).await.unwrap()["access_revision"]
        .as_i64()
        .unwrap();
    db.library_share(
        &s.owner,
        &s.w,
        &folder,
        &share(
            revision,
            "private",
            vec![],
            vec![grant(&s.agent, "viewers")],
        ),
    )
    .await
    .unwrap();
    let view = db.chat_messages(&s.member, &s.w, &theirs).await.unwrap();
    assert_eq!(
        shown(&view, &used.reply),
        "Response unavailable because source access changed."
    );
    assert!(
        shown(&view, &plain.reply).starts_with("PLAIN_ANSWER"),
        "unrelated answers stay"
    );
    let later = ask(&s, &s.member, &theirs, "Remind me?").await;
    let history = invoke(
        db,
        &later,
        "search_conversation_history",
        json!({"query":"ANSWER"}),
    )
    .await
    .unwrap();
    let ids: Vec<&str> = history["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();
    assert!(
        !ids.contains(&used.reply.as_str()),
        "agents no longer recall it"
    );
    assert!(ids.contains(&plain.reply.as_str()));
    db.chat_finish(&later, "LATER", "complete").await.unwrap();
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn running_work_continues_through_an_edit_and_stops_when_its_document_is_unshared() {
    let s = setup().await;
    let db = &s.db;
    let theirs = chat(db, &s.member, &s.w, vec![], vec![s.agent.clone()]).await;
    let job = ask(&s, &s.member, &theirs, "Check the refund window").await;
    invoke(db, &job, "read_document", json!({"id":s.returns}))
        .await
        .unwrap();
    let current = db.library_get(&s.owner, &s.w, &s.returns).await.unwrap();
    let page = |revision: i64, text: &str| SavePage {
        id: s.returns.clone(),
        request: uuid::Uuid::new_v4().to_string(),
        revision,
        title: "Returns policy".into(),
        document: json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":text}]}]}),
        folder: None,
    };
    db.library_save(
        &s.owner,
        &s.w,
        &page(current["revision"].as_i64().unwrap(), "45 days"),
    )
    .await
    .unwrap();
    assert!(
        db.chat_job_allowed(&job).await,
        "an edit doesn't stop the agent"
    );
    invoke(db, &job, "search_library", json!({"query":"refund"}))
        .await
        .unwrap();
    db.library_action(
        &s.owner,
        &s.w,
        &s.returns,
        &ItemAction {
            request: uuid::Uuid::new_v4().to_string(),
            revision: current["revision"].as_i64().unwrap() + 1,
            action: "trash".into(),
            version: None,
        },
    )
    .await
    .unwrap();
    assert!(
        !db.chat_job_allowed(&job).await,
        "trashing what it read stops it"
    );
}

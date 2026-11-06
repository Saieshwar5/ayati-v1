//! Library LB5/LB7: review dates mark documents due, for people and agents; questions
//! agents found nothing for are counted for admins without who asked.
use super::super::{tests::*, *};
use super::folders::{ask, setup};
use crate::identity::control::broker::invoke;
use crate::identity::postgres::{GapAction, Review};
use serde_json::json;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn documents_past_their_review_date_are_due_for_people_and_agents() {
    let s = setup().await;
    let db = &s.db;
    let every = |days: Option<i32>, reviewed: bool| Review {
        every_days: days,
        reviewed,
    };
    assert!(
        db.library_review_set(&s.member, &s.w, &s.returns, &every(Some(30), false))
            .await
            .is_err(),
        "editors only"
    );
    assert!(
        db.library_review_set(&s.owner, &s.w, &s.returns, &every(Some(3), false))
            .await
            .is_err()
    );
    db.library_review_set(&s.owner, &s.w, &s.returns, &every(Some(30), false))
        .await
        .unwrap();
    let review = db.library_review(&s.owner, &s.w, &s.returns).await.unwrap()["review"].clone();
    assert_eq!(
        (review["every_days"].as_i64(), review["overdue"].as_bool()),
        (Some(30), Some(false))
    );
    assert!(
        db.library_list(&s.owner, &s.w, "review", "", "", 0)
            .await
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    // Forty days later it is due: listed, flagged in the list and to agents.
    execute(
        &db.db,
        "UPDATE relay_library_reviews SET reviewed=reviewed-40*86400 WHERE workspace=$1",
        vec![s.w.clone().into()],
    )
    .await
    .unwrap();
    let due = db
        .library_list(&s.owner, &s.w, "review", "", "", 0)
        .await
        .unwrap();
    assert_eq!(due["items"][0]["title"], "Returns policy");
    assert_eq!(due["items"][0]["review_due"], true);
    let theirs = chat(db, &s.member, &s.w, vec![], vec![s.agent.clone()]).await;
    let job = ask(&s, &s.member, &theirs, "Refund window?").await;
    let found = invoke(db, &job, "search_library", json!({"query":"refund window"}))
        .await
        .unwrap();
    assert_eq!(found["results"][0]["review_due"], true);
    assert_eq!(
        invoke(db, &job, "read_document", json!({"id":s.returns}))
            .await
            .unwrap()["review_due"],
        true
    );
    db.chat_finish(&job, "30 days.", "complete").await.unwrap();
    // Marking it reviewed clears it; stopping reviews removes the interval.
    db.library_review_set(&s.owner, &s.w, &s.returns, &every(None, true))
        .await
        .unwrap();
    assert!(
        db.library_list(&s.owner, &s.w, "review", "", "", 0)
            .await
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    db.library_review_set(&s.owner, &s.w, &s.returns, &every(None, false))
        .await
        .unwrap();
    assert!(db.library_review(&s.owner, &s.w, &s.returns).await.unwrap()["review"].is_null());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn questions_agents_found_nothing_for_are_counted_for_admins_without_who_asked() {
    let s = setup().await;
    let db = &s.db;
    let theirs = chat(db, &s.member, &s.w, vec![], vec![s.agent.clone()]).await;
    for question in [
        "Parking at the office?",
        "parking at the office?",
        "Refund window",
    ] {
        let job = ask(&s, &s.member, &theirs, question).await;
        invoke(db, &job, "search_library", json!({"query":question}))
            .await
            .unwrap();
        db.chat_finish(&job, "Answered.", "complete").await.unwrap();
    }
    assert!(
        db.library_gaps(&s.member, &s.w).await.is_err(),
        "admins only"
    );
    let gaps = db.library_gaps(&s.owner, &s.w).await.unwrap()["gaps"].clone();
    assert_eq!(
        gaps.as_array().unwrap().len(),
        1,
        "only the question with no result, once"
    );
    assert_eq!(
        (
            gaps[0]["query"].as_str(),
            gaps[0]["asked"].as_i64(),
            gaps[0]["agent"].as_str()
        ),
        (Some("Parking at the office?"), Some(2), Some("Pool agent"))
    );
    assert!(gaps[0].get("requester").is_none());
    let id = gaps[0]["id"].as_str().unwrap().to_owned();
    assert!(
        db.library_gap_close(
            &s.member,
            &s.w,
            &id,
            &GapAction {
                action: "answered".into()
            }
        )
        .await
        .is_err()
    );
    db.library_gap_close(
        &s.owner,
        &s.w,
        &id,
        &GapAction {
            action: "answered".into(),
        },
    )
    .await
    .unwrap();
    assert!(
        db.library_gaps(&s.owner, &s.w).await.unwrap()["gaps"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        db.library_gap_close(
            &s.owner,
            &s.w,
            &id,
            &GapAction {
                action: "dismissed".into()
            }
        )
        .await
        .is_err(),
        "closed once"
    );
}

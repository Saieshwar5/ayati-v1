use super::{
    control_tests::{decision, request, setup},
    tests::*,
    *,
};
use crate::identity::{
    attention::{Acknowledge, Page},
    interactions::model::{Ask, Submission},
};
use serde_json::json;
pub(super) fn page(view: &str) -> Page {
    Page {
        view: view.into(),
        before: None,
    }
}
pub(super) fn ack(item: &Value, reviewed: bool) -> Acknowledge {
    Acknowledge {
        kind: item["kind"].as_str().unwrap().into(),
        id: item["id"].as_str().unwrap().into(),
        version: item["version"].as_str().unwrap().into(),
        reviewed,
    }
}
pub(super) fn ask() -> Ask {
    serde_json::from_value(json!({"key":"delivery","specification":{"version":1,"kind":"single_choice","title":"Choose a delivery method","options":[{"id":"road","label":"Road"},{"id":"air","label":"Air"}]}})).unwrap()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn attention_read_receipts_never_answer_questions_or_approve_business_actions() {
    let (db, owner, w, connection, job) = setup("ask").await;
    let question = db.ask_question(&job, &ask()).await.unwrap();
    db.chat_finish(&job, "Waiting for an answer", "complete")
        .await
        .unwrap();
    let data = db.attention(&owner, &w, &Page::default()).await.unwrap();
    assert_eq!(data["items"].as_array().unwrap().len(), 1);
    let item = &data["items"][0];
    assert_eq!(item["kind"], "question");
    assert_eq!(item["read"], false);
    assert!(!data.to_string().contains("options"));
    let input = ack(item, false);
    let (a, b) = tokio::join!(
        db.acknowledge_attention(&owner, &w, &input),
        db.acknowledge_attention(&owner, &w, &input)
    );
    assert_eq!(a.unwrap(), b.unwrap());
    assert!(db.claim_job().await.unwrap().is_none());
    assert_eq!(
        db.question(&owner, &w, question["id"].as_str().unwrap())
            .await
            .unwrap()["state"],
        "pending"
    );
    assert_eq!(
        db.attention(&owner, &w, &page("needs_you")).await.unwrap()["items"][0]["read"],
        true
    );
    assert!(
        db.acknowledge_attention(&owner, &w, &ack(item, true))
            .await
            .is_err()
    );
    let mut stale = ack(item, false);
    stale.version = "newer".into();
    assert!(db.acknowledge_attention(&owner, &w, &stale).await.is_err());
    db.answer_question(
        &owner,
        &w,
        question["id"].as_str().unwrap(),
        &Submission {
            id: uuid::Uuid::new_v4().to_string(),
            revision: 1,
            decision: "answer".into(),
            values: json!({"choices":["road"]}),
        },
    )
    .await
    .unwrap();
    assert!(db.acknowledge_attention(&owner, &w, &input).await.is_err());
    let resumed = db.claim_job().await.unwrap().unwrap();
    let proposal = db
        .control_propose(&resumed, &request(&connection, "PRIVATE_ORDER_ARGUMENT"))
        .await
        .unwrap();
    db.chat_finish(&resumed, "Review the exact action", "complete")
        .await
        .unwrap();
    let data = db.attention(&owner, &w, &page("needs_you")).await.unwrap();
    assert_eq!(data["items"].as_array().unwrap().len(), 1);
    let item = &data["items"][0];
    assert_eq!(item["kind"], "approval");
    assert!(!data.to_string().contains("PRIVATE_ORDER_ARGUMENT"));
    assert!(!data.to_string().contains("arguments"));
    db.acknowledge_attention(&owner, &w, &ack(item, false))
        .await
        .unwrap();
    assert_eq!(
        db.approval_details(&owner, &w, proposal.action["id"].as_str().unwrap())
            .await
            .unwrap()["state"],
        "pending"
    );
    assert!(db.claim_job().await.unwrap().is_none());
    let decided = db
        .control_decide(
            &owner,
            &w,
            &job.agent,
            proposal.action["id"].as_str().unwrap(),
            &decision("approve"),
        )
        .await
        .unwrap();
    db.control_finish(
        &w,
        &decided.dispatch.unwrap(),
        json!({"confirmed":true}),
        true,
    )
    .await
    .unwrap();
    assert!(
        db.attention(&owner, &w, &page("needs_you")).await.unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn attention_failed_review_is_personal_durable_and_distinct_from_read_or_task_completion() {
    let (db, owner, w, _, job) = setup("ask").await;
    db.finish_claimed_job(&job, "PRIVATE_GENERATED_OUTPUT", "failed")
        .await
        .unwrap();
    let other = user(&db, "attention-member").await;
    join(&db, &owner, &w, &other, "Member").await;
    db.channel_members(&owner, &w, &job.chat, &[owner.id.clone(), other.id.clone()])
        .await
        .unwrap();
    let data = db.attention(&owner, &w, &page("needs_you")).await.unwrap();
    let item = &data["items"][0];
    assert_eq!(item["status"], "failed");
    assert!(!data.to_string().contains("PRIVATE_GENERATED_OUTPUT"));
    assert!(
        db.attention(&other, &w, &Page::default()).await.unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        db.acknowledge_attention(&other, &w, &ack(item, true))
            .await
            .is_err()
    );
    db.acknowledge_attention(&owner, &w, &ack(item, false))
        .await
        .unwrap();
    assert_eq!(
        db.attention(&owner, &w, &Page::default()).await.unwrap()["items"][0]["reviewed"],
        false
    );
    db.acknowledge_attention(&owner, &w, &ack(item, true))
        .await
        .unwrap();
    db.acknowledge_attention(&owner, &w, &ack(item, false))
        .await
        .unwrap();
    assert!(
        db.attention(&owner, &w, &Page::default()).await.unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let recent = db.attention(&owner, &w, &page("recent")).await.unwrap();
    assert_eq!(recent["items"][0]["reviewed"], true);
    assert_eq!(recent["items"][0]["read"], true);
    // Reconnect to this fixture's schema, rather than treating a different empty
    // database as restart evidence.
    let schema: String = query(&db.db, "SELECT current_schema() AS name", vec![])
        .await
        .unwrap()
        .unwrap()
        .try_get("", "name")
        .unwrap();
    let mut url = reqwest::Url::parse(&std::env::var("RELAY_TEST_DATABASE_URL").unwrap()).unwrap();
    url.query_pairs_mut()
        .append_pair("options", &format!("-c search_path={schema}"));
    let reopened = PgStore::connect(url.as_str()).await.unwrap();
    reopened.verify_schema().await.unwrap();
    assert!(
        reopened
            .attention(&owner, &w, &Page::default())
            .await
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        reopened
            .attention(&owner, &w, &page("recent"))
            .await
            .unwrap(),
        recent
    );
    assert!(reopened.claim_job().await.unwrap().is_none());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn attention_unconfirmed_action_remains_needed_after_read_without_repeating_the_write() {
    let (db, owner, w, connection, job) = setup("allow").await;
    let proposed = db
        .control_propose(&job, &request(&connection, "one"))
        .await
        .unwrap();
    let dispatch = proposed.dispatch.unwrap();
    assert!(
        db.attention(&owner, &w, &Page::default()).await.unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    execute(
        &db.db,
        "UPDATE relay_control_actions SET updated=updated-61 WHERE workspace=$1 AND id=$2",
        vec![w.clone().into(), dispatch.id.clone().into()],
    )
    .await
    .unwrap();
    let held = db.attention(&owner, &w, &Page::default()).await.unwrap();
    assert_eq!(held["items"][0]["status"], "uncertain");
    db.acknowledge_attention(&owner, &w, &ack(&held["items"][0], false))
        .await
        .unwrap();
    db.control_finish(&w, &dispatch, json!({"message":"Unconfirmed"}), false)
        .await
        .unwrap();
    let unknown = db.attention(&owner, &w, &Page::default()).await.unwrap();
    assert_eq!(unknown["items"][0]["read"], true);
    assert_eq!(unknown["items"][0]["status"], "uncertain");
    assert!(
        db.control_propose(&job, &request(&connection, "different"))
            .await
            .is_err()
    );
    assert!(
        db.acknowledge_attention(&owner, &w, &ack(&unknown["items"][0], true))
            .await
            .is_err()
    );
}

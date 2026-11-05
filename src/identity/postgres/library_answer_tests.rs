//! LB5: an answer a person gave in Chat, saved to a folder an agent was given, answers
//! the same question next time; only people in the conversation can save from it.
use super::super::tests::*;
use super::folders::{ask, setup};
use crate::identity::control::broker::invoke;
use crate::identity::postgres::SaveAnswer;
use serde_json::json;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn an_answer_saved_from_chat_answers_the_agent_next_time() {
    let s = setup().await;
    let db = &s.db;
    // The member asks; the agent finds nothing about parking.
    let conversation = chat(
        db,
        &s.member,
        &s.w,
        vec![s.owner.id.clone()],
        vec![s.agent.clone()],
    )
    .await;
    let job = ask(&s, &s.member, &conversation, "Where do visitors park?").await;
    let none = invoke(
        db,
        &job,
        "search_library",
        json!({"query":"visitor parking"}),
    )
    .await
    .unwrap();
    assert!(none["results"].as_array().unwrap().is_empty());
    db.chat_finish(&job, "I don't know.", "complete")
        .await
        .unwrap();
    // The owner answers in the conversation and saves the answer to Policies.
    let answer = "Visitors park in **bay C**, behind the warehouse.";
    db.chat_send(&s.owner, &s.w, &conversation, &message(answer, None))
        .await
        .unwrap();
    let said: String = super::super::rows(
        &db.db,
        "SELECT id FROM relay_chat_messages WHERE chat=$1 AND content=$2",
        vec![conversation.clone().into(), answer.into()],
    )
    .await
    .unwrap()[0]
        .try_get("", "id")
        .unwrap();
    let policies = db.library_get(&s.owner, &s.w, &s.returns).await.unwrap()["folder"]
        .as_str()
        .unwrap()
        .to_owned();
    let save = |who: &str| SaveAnswer {
        id: uuid::Uuid::new_v4().to_string(),
        request: uuid::Uuid::new_v4().to_string(),
        folder: Some(policies.clone()),
        title: "Where do visitors park?".into(),
        text: answer.into(),
        message: who.into(),
    };
    // Someone outside the conversation can't save from it.
    let outsider = user(db, "answer-outsider").await;
    join(db, &s.owner, &s.w, &outsider, "Member").await;
    assert!(
        db.library_save_answer(&outsider, &s.w, &save(&said))
            .await
            .is_err()
    );
    let input = save(&said);
    db.library_save_answer(&s.owner, &s.w, &input)
        .await
        .unwrap();
    // A retry of the same save is the same page, audited once.
    db.library_save_answer(&s.owner, &s.w, &input)
        .await
        .unwrap();
    let saved = db.library_get(&s.owner, &s.w, &input.id).await.unwrap();
    assert_eq!(saved["folder"], policies.as_str());
    assert_eq!(saved["document"]["content"][0]["type"], "paragraph");
    let audits: i64 = super::super::rows(
        &db.db,
        "SELECT count(*) AS n FROM relay_audit WHERE action='library_answer_saved' AND target=$1",
        vec![input.id.clone().into()],
    )
    .await
    .unwrap()[0]
        .try_get("", "n")
        .unwrap();
    assert_eq!(audits, 1);
    // Next time, the agent finds the saved answer for the member.
    let job = ask(&s, &s.member, &conversation, "Where do visitors park?").await;
    let found = invoke(db, &job, "search_library", json!({"query":"visitors park"}))
        .await
        .unwrap();
    assert_eq!(found["results"][0]["title"], "Where do visitors park?");
    assert!(
        found["results"][0]["preview"]
            .as_str()
            .unwrap()
            .contains("bay C")
    );
}

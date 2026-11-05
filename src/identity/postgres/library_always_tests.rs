//! Library LB2c: an agent's managers choose up to five documents it always has; each
//! request includes only those usable for its audience, and they fence the reply.
use super::super::{tests::*, *};
use super::folders::{ask, grant, setup, share};
use crate::identity::postgres::AlwaysRead;
use serde_json::json;

fn choose(items: &[&str], expected: &[&str]) -> AlwaysRead {
    AlwaysRead {
        items: items.iter().map(|s| s.to_string()).collect(),
        expected: expected.iter().map(|s| s.to_string()).collect(),
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn always_read_documents_follow_the_audience_of_each_request() {
    let s = setup().await;
    let db = &s.db;
    let listed = db.library_always(&s.owner, &s.w, &s.agent).await.unwrap();
    assert_eq!(listed["editable"], true);
    assert_eq!(
        listed["given"].as_array().unwrap().len(),
        2,
        "both documents the agent was given"
    );
    // A member who can talk to the agent but doesn't manage it can't choose.
    assert!(
        db.library_always_save(&s.member, &s.w, &s.agent, &choose(&[&s.returns], &[]))
            .await
            .is_err()
    );
    let other = uuid::Uuid::new_v4().to_string();
    assert!(
        db.library_always_save(&s.owner, &s.w, &s.agent, &choose(&[&other], &[]))
            .await
            .is_err(),
        "only given documents"
    );
    db.library_always_save(
        &s.owner,
        &s.w,
        &s.agent,
        &choose(&[&s.returns, &s.salaries], &[]),
    )
    .await
    .unwrap();
    assert!(
        db.library_always_save(&s.owner, &s.w, &s.agent, &choose(&[&s.returns], &[]))
            .await
            .is_err(),
        "a save from an older list is refused"
    );
    let six: Vec<String> = (0..6).map(|_| uuid::Uuid::new_v4().to_string()).collect();
    let six: Vec<&str> = six.iter().map(String::as_str).collect();
    assert!(
        db.library_always_save(
            &s.owner,
            &s.w,
            &s.agent,
            &choose(&six, &[&s.returns, &s.salaries])
        )
        .await
        .is_err()
    );
    let said: String = query(
        &db.db,
        "SELECT summary FROM relay_audit WHERE workspace=$1 AND action='library_always_read'",
        vec![s.w.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "summary")
    .unwrap();
    assert_eq!(said, "Pool agent always reads: Returns policy, Salaries");
    // A colleague's request gets the staff policy, not HR.
    let theirs = chat(db, &s.member, &s.w, vec![], vec![s.agent.clone()]).await;
    let job = ask(&s, &s.member, &theirs, "Hello").await;
    let given = db.library_always_context(&job).await.unwrap();
    assert!(given.contains("## Returns policy") && given.contains("refund window is 30 days"));
    assert!(!given.contains("40k"));
    db.chat_finish(&job, "Hi.", "complete").await.unwrap();
    let answer: String = query(
        &db.db,
        "SELECT content FROM relay_chat_messages WHERE id=$1",
        vec![job.reply.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "content")
    .unwrap();
    assert_eq!(answer, "Hi.", "always-read documents aren't cited");
    // The owner's private request gets both; sharing HR differently stops it.
    let private = chat(db, &s.owner, &s.w, vec![], vec![s.agent.clone()]).await;
    let mine = ask(&s, &s.owner, &private, "Hello").await;
    assert!(
        db.library_always_context(&mine)
            .await
            .unwrap()
            .contains("40k")
    );
    assert!(db.chat_job_allowed(&mine).await);
    db.library_share(
        &s.owner,
        &s.w,
        &s.hr,
        &share(2, "private", vec![], vec![grant(&s.agent, "staff")]),
    )
    .await
    .unwrap();
    assert!(!db.chat_job_allowed(&mine).await);
    // A colleague who can open neither document sees how many there are, not their names.
    let seen = db.library_always(&s.member, &s.w, &s.agent).await.unwrap();
    assert_eq!(seen["editable"], false);
    assert_eq!(
        (seen["chosen"].as_i64(), seen["hidden"].as_i64()),
        (Some(2), Some(2))
    );
    assert_eq!(seen["items"], json!([]));
    assert_eq!(seen["given"], json!([]));
}

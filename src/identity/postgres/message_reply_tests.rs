//! A message can quote one earlier message of its conversation; the agent
//! asked by the reply sees the quoted message even outside its recent window (C4).
use super::{tests::*, *};
use crate::identity::chat::model::NewMessage;

fn reply(text: &str, agent: Option<&str>, quoted: &str) -> NewMessage {
    NewMessage {
        reply_to: Some(quoted.into()),
        ..message(text, agent)
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn replies_quote_one_message_of_the_same_conversation_and_reach_the_agent() {
    let db = fixture().await;
    let owner = user(&db, "quoted-replies").await;
    let w = workspace(&db, &owner).await;
    let (c, agent) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    let (other, _) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    let original = message("Three orders need a delivery slot", None);
    db.chat_send(&owner, &w, &c, &original).await.unwrap();
    for n in 0..4 {
        db.chat_send(&owner, &w, &c, &message(&format!("Later note {n}"), None))
            .await
            .unwrap();
    }

    // A quote must come from the same conversation.
    let foreign = reply("Wrong place", None, &original.id);
    let error = db
        .chat_send(&owner, &w, &other, &foreign)
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Quoted message is not in this conversation")
    );

    let answer = reply("Yes, start with Kapoor", Some(&agent), &original.id);
    let job = db
        .chat_send(&owner, &w, &c, &answer)
        .await
        .unwrap()
        .expect("agent asked");
    let history = db.chat_messages(&owner, &w, &c).await.unwrap();
    let sent = history["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == answer.id)
        .unwrap();
    assert_eq!(sent["reply_to"], original.id.as_str());

    // Reusing the ID with a different quote is refused; an exact retry is accepted.
    let other_quote = NewMessage {
        reply_to: None,
        ..reply("Yes, start with Kapoor", Some(&agent), &original.id)
    };
    let mut changed = other_quote;
    changed.id = answer.id.clone();
    assert!(db.chat_send(&owner, &w, &c, &changed).await.is_err());

    // The quoted message is in the agent's evidence even with a one-message window.
    let tx = db.db.begin().await.unwrap();
    let evidence = super::conversation_context::recent(
        &tx,
        super::conversation_context::RecentScope {
            workspace: &w,
            chat: &c,
            agent: &agent,
            thread: None,
            main_only: false,
            since_join: false,
            limit: 1,
            before: i64::MAX,
            request: &answer.id,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let ids: Vec<_> = evidence
        .iter()
        .map(|m| m["id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(ids, vec![original.id.clone(), answer.id.clone()]);
    assert_eq!(evidence[1]["reply_to"], original.id.as_str());
    assert!(
        job.prompt.contains(&original.id),
        "the queued prompt carries the quoted message"
    );
}

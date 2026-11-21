//! Asking your assistant from a message box (2026-11-21): the one message a
//! draft answers, a draft changed on request, and a mention never posted.
use super::super::super::{tests::*, *};
use super::DraftRequest;
use super::tests::{claimed, direct_with, one};

fn ask(chat: &str, instruction: &str) -> DraftRequest {
    DraftRequest {
        id: uuid::Uuid::new_v4().to_string(),
        chat: chat.into(),
        thread: None,
        instruction: instruction.into(),
        message: None,
        previous: None,
    }
}
/// What the assistant was asked for the request `id`.
async fn asked(db: &PgStore, id: &str) -> String {
    one(
        db,
        "SELECT content FROM relay_chat_messages WHERE id=$1",
        vec![id.into()],
    )
    .await
    .as_str()
    .unwrap()
    .to_owned()
}

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn an_asked_draft_answers_one_message_and_is_changed_on_request() {
    let mut db = fixture().await;
    db.pooled = true;
    let ravi = user(&db, "ravi").await;
    let priya = user(&db, "priya").await;
    let arjun = user(&db, "arjun").await;
    let w = workspace(&db, &ravi).await;
    join(&db, &ravi, &w, &priya, "Member").await;
    join(&db, &ravi, &w, &arjun, "Member").await;
    db.ensure_assistant(&ravi, &w, "fixture", "openai")
        .await
        .unwrap();
    let chat = direct_with(&db, &priya, &w, &ravi).await;
    let first = message("Can we meet on Friday at 3?", None);
    db.chat_send(&priya, &w, &chat, &first).await.unwrap();
    let later = message("And who brings the samples?", None);
    db.chat_send(&priya, &w, &chat, &later).await.unwrap();

    // The message to answer is named apart from the conversation.
    let mut request = ask(&chat, "");
    request.message = Some(first.id.clone());
    let (_, job) = db.draft_request(&ravi, &w, &request).await.unwrap();
    let content = asked(&db, &request.id).await;
    assert!(content.contains("Write what I would most likely answer."));
    let line = content
        .lines()
        .find(|line| line.starts_with("Answer this message in particular: "))
        .expect("the message to answer");
    assert!(line.ends_with("Can we meet on Friday at 3?"), "{line}");

    // A message from a conversation its person can't read is not one to answer.
    let elsewhere = direct_with(&db, &priya, &w, &arjun).await;
    let private = message("Ravi must not see this.", None);
    db.chat_send(&priya, &w, &elsewhere, &private)
        .await
        .unwrap();
    let mut wrong = ask(&chat, "");
    wrong.message = Some(private.id.clone());
    let refused = db.draft_request(&ravi, &w, &wrong).await.unwrap_err();
    assert!(refused.to_string().contains("not available here"));

    // What its person typed is passed on as what they asked for.
    let request = ask(&chat, "go through the orders and answer her");
    db.draft_request(&ravi, &w, &request).await.unwrap();
    let content = asked(&db, &request.id).await;
    assert!(content.contains("What I asked for: go through the orders and answer her"));
    assert!(!content.contains("Answer this message in particular"));

    // A draft is changed only once it is written, and only with a change named.
    let mut change = ask(&chat, "shorter");
    change.previous = Some(job.reply.clone());
    let early = db.draft_request(&ravi, &w, &change).await.unwrap_err();
    assert!(early.to_string().contains("not finished"));
    let job = claimed(&db, &job).await;
    db.chat_finish(&job, "Yes, Friday at 3 works for me.", "complete")
        .await
        .unwrap();
    db.draft_request(&ravi, &w, &change).await.unwrap();
    let content = asked(&db, &change.id).await;
    assert!(content.contains("Your draft:\nYes, Friday at 3 works for me."));
    assert!(content.contains("The change I want: shorter"));
    let mut silent = ask(&chat, " ");
    silent.previous = Some(job.reply.clone());
    assert!(db.draft_request(&ravi, &w, &silent).await.is_err());

    // Nobody else changes it: Priya's assistant never wrote that draft.
    db.ensure_assistant(&priya, &w, "fixture", "openai")
        .await
        .unwrap();
    let mut theirs = ask(&chat, "longer");
    theirs.previous = Some(job.reply.clone());
    let refused = db.draft_request(&priya, &w, &theirs).await.unwrap_err();
    assert!(refused.to_string().contains("Draft not found"));

    assert_eq!(
        one(
            &db,
            "SELECT count(*) FROM relay_chat_messages WHERE chat=$1",
            vec![chat.clone().into()]
        )
        .await,
        2,
        "nothing was posted in the chat"
    );
}

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn a_message_that_mentions_your_assistant_is_never_posted() {
    let mut db = fixture().await;
    db.pooled = true;
    let ravi = user(&db, "ravi").await;
    let priya = user(&db, "priya").await;
    let w = workspace(&db, &ravi).await;
    join(&db, &ravi, &w, &priya, "Member").await;
    let mine = db
        .ensure_assistant(&ravi, &w, "fixture", "openai")
        .await
        .unwrap();
    let chat = direct_with(&db, &priya, &w, &ravi).await;
    let text = format!("[@Tara](mention:agent:{mine}) tell her the order is late, not why");

    // Sent as an ordinary message, what Ravi asked would be posted for Priya.
    let refused = db
        .chat_send(&ravi, &w, &chat, &message(&text, None))
        .await
        .unwrap_err();
    assert!(
        refused.to_string().contains("This asks your assistant"),
        "{refused}"
    );
    // Asked outright, his assistant is still not in this conversation.
    assert!(
        db.chat_send(&ravi, &w, &chat, &message(&text, Some(&mine)))
            .await
            .is_err()
    );
    // Someone else's message is refused without learning whose assistant it is.
    let other = db
        .chat_send(&priya, &w, &chat, &message(&text, None))
        .await
        .unwrap_err();
    assert!(!other.to_string().contains("assistant"), "{other}");
    assert_eq!(
        one(
            &db,
            "SELECT count(*) FROM relay_chat_messages WHERE chat=$1",
            vec![chat.clone().into()]
        )
        .await,
        0
    );
}

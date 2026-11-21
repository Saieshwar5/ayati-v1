//! Files in a forward: the forwarded message's own files are copied into the
//! direct chat, your documents are attached, and a retry stores nothing twice.
use super::super::super::{tests::*, *};
use super::copy_id;
use super::tests::{agent, direct_with, forward, named, stored};
use crate::identity::chat::model::NewChat;
use serde_json::json;

/// A file uploaded to `chat` by `who`, as the upload route saves it.
async fn file(db: &PgStore, who: &User, w: &str, chat: &str, name: &str, mime: &str) -> String {
    let object: crate::identity::collaboration::Object = serde_json::from_value(json!({
        "id": uuid::Uuid::new_v4().to_string(), "name": name, "mime": mime, "size": 10, "checksum": "a".repeat(64),
    }))
    .unwrap();
    db.save_object(who, w, chat, &object, true).await.unwrap();
    object.id
}

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn a_forward_carries_the_files_you_choose_and_your_own_documents() {
    let db = fixture().await;
    let ravi = user(&db, "ravi").await;
    let priya = user(&db, "priya").await;
    let arjun = user(&db, "arjun").await;
    let w = workspace(&db, &ravi).await;
    join(&db, &ravi, &w, &priya, "Member").await;
    named(&db, &priya, "Priya Sharma").await;
    join(&db, &ravi, &w, &arjun, "Member").await;
    let finance = agent(&db, &ravi, &w, "Finance Desk", "everyone").await;
    let from_priya = direct_with(&db, &priya, &w, &ravi).await;
    let payslip = file(
        &db,
        &priya,
        &w,
        &from_priya,
        "payslip.pdf",
        "application/octet-stream",
    )
    .await;
    let statement = file(
        &db,
        &priya,
        &w,
        &from_priya,
        "statement.pdf",
        "application/octet-stream",
    )
    .await;
    let elsewhere = file(
        &db,
        &priya,
        &w,
        &from_priya,
        "unsent.pdf",
        "application/octet-stream",
    )
    .await;
    let mut asked = message("Here is the payslip and my bank statement.", None);
    asked.attachments = vec![payslip.clone(), statement.clone()];
    db.chat_send(&priya, &w, &from_priya, &asked).await.unwrap();

    // Only files of the forwarded message, each once.
    for files in [
        vec![elsewhere.clone()],
        vec![payslip.clone(), payslip.clone()],
    ] {
        let mut input = forward("agent", &finance, "Check this");
        input.files = files;
        let refused = db
            .forward_message(&ravi, &w, &from_priya, &asked.id, &input)
            .await
            .unwrap_err();
        assert!(refused.to_string().contains("not part of the message"));
    }

    // Ravi carries one of the two files and adds a document of his own.
    let mut input = forward("agent", &finance, "Find out why it is short.");
    input.files = vec![payslip.clone()];
    let sources = db
        .forward_sources(&ravi, &w, &from_priya, &asked.id, &input)
        .await
        .unwrap();
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].name, "payslip.pdf");
    let chat = db
        .chat_create(
            &ravi,
            &w,
            &NewChat {
                id: input.chat.clone(),
                name: "Finance Desk".into(),
                kind: "direct".into(),
                description: String::new(),
                users: vec![],
                agents: vec![finance.clone()],
            },
        )
        .await
        .unwrap();
    let policy = file(
        &db,
        &ravi,
        &w,
        &chat,
        "leave-policy.pdf",
        "application/octet-stream",
    )
    .await;
    input.attachments = vec![policy.clone()];
    let (went_to, job) = db
        .forward_message(&ravi, &w, &from_priya, &asked.id, &input)
        .await
        .unwrap();
    assert_eq!(went_to, chat);
    let copy = copy_id(&input.id, &payslip);
    let sent = stored(&db, &input.id).await;
    assert_eq!(sent["attachments"], json!([copy, policy]));
    assert_eq!(
        sent["forwarded"]["files"],
        json!([{"id": copy, "name": "payslip.pdf", "mime": "application/octet-stream", "size": 10}])
    );
    // The copy is Ravi's file in the new chat; Priya's file stays where it was.
    assert_eq!(
        db.object(&ravi, &w, &copy).await.unwrap().name,
        "payslip.pdf"
    );
    assert!(db.object(&arjun, &w, &copy).await.is_err());
    assert!(db.object(&priya, &w, &payslip).await.is_ok());
    // The agent is told which files came with the copy.
    let prompt = job.expect("the agent is asked").prompt;
    assert!(prompt.contains("leave-policy.pdf") && prompt.contains("payslip.pdf"));
    assert!(prompt.contains("files lists the files that came with it"));

    // A retried request sends it once, with the same copies.
    let (again, retried) = db
        .forward_message(&ravi, &w, &from_priya, &asked.id, &input)
        .await
        .unwrap();
    assert_eq!(again, chat);
    assert!(retried.is_none());
    // The history the client reads lists both files.
    let page = db.chat_messages(&ravi, &w, &chat).await.unwrap();
    let shown = page["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == input.id.as_str())
        .unwrap();
    assert_eq!(shown["attachments"].as_array().unwrap().len(), 2);
    assert_eq!(shown["forwarded"]["files"][0]["id"], copy.as_str());

    // Deleting the forward takes its copies out of reach of the reader.
    db.delete_message(&ravi, &w, &chat, &input.id)
        .await
        .unwrap();
    assert_eq!(stored(&db, &input.id).await["attachments"], json!([]));
}

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn a_message_that_is_only_files_forwards_its_files_not_a_placeholder() {
    let db = fixture().await;
    let ravi = user(&db, "ravi").await;
    let priya = user(&db, "priya").await;
    let w = workspace(&db, &ravi).await;
    join(&db, &ravi, &w, &priya, "Member").await;
    named(&db, &priya, "Priya Sharma").await;
    let arjun = user(&db, "arjun").await;
    join(&db, &ravi, &w, &arjun, "Member").await;
    let from_priya = direct_with(&db, &priya, &w, &ravi).await;
    let note = file(
        &db,
        &priya,
        &w,
        &from_priya,
        "voice-message.ogg",
        "audio/ogg",
    )
    .await;
    execute(
        &db.db,
        "UPDATE relay_object_transcripts SET status='ready',text='Call me about the invoice',duration_ms=2000,finished_ms=1 WHERE workspace=$1 AND object=$2",
        vec![w.clone().into(), note.clone().into()],
    )
    .await
    .unwrap();
    let mut shared = message("Shared files", None);
    shared.attachments = vec![note.clone()];
    db.chat_send(&priya, &w, &from_priya, &shared)
        .await
        .unwrap();

    // With no file chosen there is nothing to forward.
    let nothing = db
        .forward_message(
            &ravi,
            &w,
            &from_priya,
            &shared.id,
            &forward("user", &arjun.id, "FYI"),
        )
        .await
        .unwrap_err();
    assert!(nothing.to_string().contains("at least one of its files"));

    let mut input = forward("user", &arjun.id, "");
    input.files = vec![note.clone()];
    let (chat, _) = db
        .forward_message(&ravi, &w, &from_priya, &shared.id, &input)
        .await
        .unwrap();
    let sent = stored(&db, &input.id).await;
    assert_eq!(sent["forwarded"]["text"], "");
    assert_eq!(sent["forwarded"]["files"][0]["name"], "voice-message.ogg");
    // The copy keeps the transcript, so its reader need not wait for it again.
    let copy = copy_id(&input.id, &note);
    let page = db.chat_messages(&arjun, &w, &chat).await.unwrap();
    let shown = page["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == input.id.as_str())
        .unwrap();
    assert_eq!(shown["attachments"][0]["id"], copy.as_str());
    assert_eq!(
        shown["attachments"][0]["transcript"],
        "Call me about the invoice"
    );

    // Forwarding that bare forward on carries the file again, named afresh.
    let mut onward = forward("user", &priya.id, "");
    onward.files = vec![copy.clone()];
    db.forward_message(&arjun, &w, &chat, &input.id, &onward)
        .await
        .unwrap();
    let carried = stored(&db, &onward.id).await;
    assert_eq!(carried["forwarded"]["name"], "Priya Sharma");
    assert_eq!(
        carried["forwarded"]["files"][0]["id"],
        copy_id(&onward.id, &copy).as_str()
    );
}

//! The file tools find a file only where the message that shared it may be
//! read: in the task's own conversation, sent, not deleted, before the request.
use super::super::super::{executor_slot_tests::agent, tests::*, *};
use super::super::contents::tests::{file, read_as, sharing};
use crate::identity::{chat::model::Job, control::broker::invoke};
use serde_json::json;

async fn list(db: &PgStore, job: &Job, input: Value) -> Value {
    invoke(db, job, "list_conversation_files", input)
        .await
        .unwrap()
}
async fn read(db: &PgStore, job: &Job, id: &str, offset: i64) -> Value {
    invoke(
        db,
        job,
        "read_conversation_file",
        json!({"file":id,"offset":offset}),
    )
    .await
    .unwrap()
}
fn names(page: &Value) -> Vec<String> {
    page["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn files_are_found_only_in_the_conversation_that_shared_them() {
    let db = fixture().await;
    let owner = user(&db, "files-scope-owner").await;
    let w = workspace(&db, &owner).await;
    let (c, a) = agent(&db, &owner, &w).await;
    let other = chat(&db, &owner, &w, vec![], vec![a.clone()]).await;
    let contract = file("contract.pdf", "application/pdf");
    db.save_object(&owner, &w, &c, &contract, false)
        .await
        .unwrap();
    let sent = sharing("Signed contract", None, &[&contract]);
    db.chat_send(&owner, &w, &c, &sent).await.unwrap();
    read_as(&db, "pdf", "Contract with Acme. Notice period: 60 days.").await;
    // Uploaded here but never sent: only its uploader knows of it.
    let draft = file("draft.txt", "text/plain");
    db.save_object(&owner, &w, &c, &draft, false).await.unwrap();
    read_as(&db, "text", "Unsent draft").await;

    db.chat_send(
        &owner,
        &w,
        &c,
        &message("What is the notice period?", Some(&a)),
    )
    .await
    .unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    let page = list(&db, &job, json!({})).await;
    assert_eq!(names(&page), ["contract.pdf"]);
    let listed = &page["files"][0];
    assert_eq!(listed["file_id"], contract.id);
    assert_eq!(listed["reading"], "ready");
    assert_eq!(listed["kind"], "pdf");
    assert_eq!(listed["message"]["id"], sent.id);
    assert_eq!(listed["message"]["author"]["id"], owner.id);
    let opened = read(&db, &job, &contract.id, 0).await;
    assert_eq!(
        opened["file"]["text"],
        "Contract with Acme. Notice period: 60 days."
    );
    assert_eq!(opened["next_offset"], Value::Null);
    assert_eq!(read(&db, &job, &draft.id, 0).await["state"], "not_found");
    db.finish_claimed_job(&job, "Sixty days", "complete")
        .await
        .unwrap();

    // The same agent in another conversation can't reach it, even by its ID.
    db.chat_send(
        &owner,
        &w,
        &other,
        &message("What did the contract say?", Some(&a)),
    )
    .await
    .unwrap();
    let elsewhere = db.claim_job().await.unwrap().unwrap();
    assert_eq!(elsewhere.chat, other);
    assert_eq!(
        names(&list(&db, &elsewhere, json!({})).await),
        [] as [&str; 0]
    );
    let foreign = read(&db, &elsewhere, &contract.id, 0).await;
    let missing = read(&db, &elsewhere, &uuid::Uuid::new_v4().to_string(), 0).await;
    assert_eq!(foreign, missing, "a foreign file looks like a missing one");
    assert_eq!(foreign["state"], "not_found");
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn deleted_and_later_files_are_not_found() {
    let db = fixture().await;
    let owner = user(&db, "files-delete-owner").await;
    let w = workspace(&db, &owner).await;
    let (c, a) = agent(&db, &owner, &w).await;
    let payslip = file("payslip.txt", "text/plain");
    db.save_object(&owner, &w, &c, &payslip, false)
        .await
        .unwrap();
    let sent = sharing("My payslip", None, &[&payslip]);
    db.chat_send(&owner, &w, &c, &sent).await.unwrap();
    read_as(&db, "text", "Net pay 51,200").await;
    db.delete_message(&owner, &w, &c, &sent.id).await.unwrap();
    let request = message("Which files did I send?", Some(&a));
    db.chat_send(&owner, &w, &c, &request).await.unwrap();
    // Sent after the request: not part of what it asked about.
    let later = file("later.txt", "text/plain");
    db.save_object(&owner, &w, &c, &later, false).await.unwrap();
    db.chat_send(&owner, &w, &c, &sharing("One more", None, &[&later]))
        .await
        .unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    assert_eq!(names(&list(&db, &job, json!({})).await), [] as [&str; 0]);
    assert_eq!(read(&db, &job, &payslip.id, 0).await["state"], "not_found");
    assert_eq!(read(&db, &job, &later.id, 0).await["state"], "not_found");
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn people_in_a_shared_room_can_ask_about_files_posted_there() {
    let db = fixture().await;
    let owner = user(&db, "files-room-owner").await;
    let member = user(&db, "files-room-member").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &member, "Member").await;
    let (_, a) = agent(&db, &owner, &w).await;
    db.chat_grant(&owner, &w, &member.id, &a, true)
        .await
        .unwrap();
    let room = chat(&db, &owner, &w, vec![member.id.clone()], vec![a.clone()]).await;
    let rota = file("rota.txt", "text/plain");
    db.save_object(&owner, &w, &room, &rota, false)
        .await
        .unwrap();
    db.chat_send(
        &owner,
        &w,
        &room,
        &sharing("This week's rota", None, &[&rota]),
    )
    .await
    .unwrap();
    read_as(&db, "text", "Monday: Priya. Tuesday: Ravi.").await;
    // Everyone in the room already sees the file, so the agent may use it for them.
    db.chat_send(&member, &w, &room, &message("Who is on Tuesday?", Some(&a)))
        .await
        .unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    assert_eq!(job.requester, member.id);
    let page = list(&db, &job, json!({"name":"ROTA"})).await;
    assert_eq!(names(&page), ["rota.txt"]);
    assert_eq!(page["files"][0]["message"]["author"]["id"], owner.id);
    assert_eq!(
        read(&db, &job, &rota.id, 0).await["file"]["text"],
        "Monday: Priya. Tuesday: Ravi."
    );
    assert_eq!(
        names(&list(&db, &job, json!({"name":"invoice"})).await),
        [] as [&str; 0]
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn files_page_newest_first_and_read_in_chunks() {
    let db = fixture().await;
    let owner = user(&db, "files-page-owner").await;
    let w = workspace(&db, &owner).await;
    let (c, a) = agent(&db, &owner, &w).await;
    for n in 1..=12 {
        let upload = file(&format!("week-{n:02}.txt"), "text/plain");
        db.save_object(&owner, &w, &c, &upload, false)
            .await
            .unwrap();
        db.chat_send(&owner, &w, &c, &sharing("Report", None, &[&upload]))
            .await
            .unwrap();
        read_as(&db, "text", &"r".repeat(9_000)).await;
    }
    let voice = file("voice-message.webm", "audio/webm");
    db.save_object(&owner, &w, &c, &voice, false).await.unwrap();
    db.chat_send(&owner, &w, &c, &sharing("Shared files", None, &[&voice]))
        .await
        .unwrap();
    db.chat_send(&owner, &w, &c, &message("Summarise the reports", Some(&a)))
        .await
        .unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    let first = list(&db, &job, json!({})).await;
    let mut expected: Vec<String> = vec!["voice-message.webm".into()];
    expected.extend((4..=12).rev().map(|n| format!("week-{n:02}.txt")));
    assert_eq!(names(&first), expected, "ten messages' files, newest first");
    assert_eq!(first["files"][0]["transcript_status"], "not_enabled");
    assert_eq!(first["files"][0].get("reading"), None);
    let second = list(&db, &job, json!({"before":first["next_before"]})).await;
    assert_eq!(
        names(&second),
        ["week-03.txt", "week-02.txt", "week-01.txt"]
    );
    assert_eq!(second["next_before"], Value::Null);

    let id = first["files"][1]["file_id"].as_str().unwrap().to_owned();
    let start = read(&db, &job, &id, 0).await;
    assert_eq!(start["file"]["characters"], 9_000);
    assert_eq!(start["file"]["text"].as_str().unwrap().len(), 4_000);
    assert_eq!(start["next_offset"], 4_000);
    let end = read(&db, &job, &id, 8_000).await;
    assert_eq!(end["file"]["text"].as_str().unwrap().len(), 1_000);
    assert_eq!(end["next_offset"], Value::Null);
    let beyond = invoke(
        &db,
        &job,
        "read_conversation_file",
        json!({"file":id,"offset":9_001}),
    )
    .await;
    assert!(beyond.is_err(), "an offset past the end is refused");
    // A file without text says why.
    let unheard = read(&db, &job, &voice.id, 0).await;
    assert_eq!(unheard["file"]["transcript_status"], "not_enabled");
    assert_eq!(unheard["file"]["text"], Value::Null);
}

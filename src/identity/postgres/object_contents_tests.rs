//! Chat file readings: queued on upload, taken once, saved or given up for good,
//! and shown to people only with the message they belong to.
use super::super::super::{
    tests::{chat, fixture, message, user, workspace},
    *,
};
use super::{Claimed, Outcome};
use crate::identity::{chat::model::NewMessage, collaboration::Object};
use serde_json::json;

pub(in crate::identity::postgres) fn file(name: &str, mime: &str) -> Object {
    Object {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.into(),
        mime: mime.into(),
        size: 2048,
        checksum: "filehash".into(),
    }
}
pub(in crate::identity::postgres) fn sharing(
    text: &str,
    agent: Option<&str>,
    files: &[&Object],
) -> NewMessage {
    let mut input = message(text, agent);
    input.attachments = files.iter().map(|file| file.id.clone()).collect();
    input
}
/// Takes the one waiting file and saves what was "read" from it.
pub(in crate::identity::postgres) async fn read_as(db: &PgStore, kind: &'static str, text: &str) {
    let held = take(db).await;
    let outcome = Outcome::Read {
        kind,
        text: text.into(),
        truncated: false,
    };
    assert!(db.finish_reading(&held, outcome).await.unwrap());
}

/// The reading's status and tries so far, if it was queued.
async fn queued(db: &PgStore, w: &str, object: &Object) -> Option<(String, i16)> {
    let row = query(
        &db.db,
        "SELECT status,attempts FROM relay_object_contents WHERE workspace=$1 AND object=$2",
        vec![w.into(), object.id.clone().into()],
    )
    .await
    .unwrap()?;
    Some((
        row.try_get("", "status").unwrap(),
        row.try_get("", "attempts").unwrap(),
    ))
}
async fn lease_ends(db: &PgStore, w: &str) {
    execute(
        &db.db,
        "UPDATE relay_object_contents SET lease_until_ms=0 WHERE workspace=$1",
        vec![w.into()],
    )
    .await
    .unwrap();
}
async fn take(db: &PgStore) -> Claimed {
    let mut taken = db.claim_readings(1).await.unwrap();
    assert_eq!(taken.len(), 1);
    taken.remove(0)
}
/// The first attachment of a message, as a person reading the conversation sees it.
async fn seen(db: &PgStore, person: &User, w: &str, c: &str, id: &str) -> Result<Value> {
    let page = db.chat_page(person, w, c, None).await?;
    let message = page["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == id)
        .cloned()
        .context("Message not shown")?;
    Ok(message["attachments"][0].clone())
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn uploads_queue_one_reading_that_one_controller_takes() {
    let db = fixture().await;
    let owner = user(&db, "reading-queue-owner").await;
    let w = workspace(&db, &owner).await;
    let c = chat(&db, &owner, &w, vec![], vec![]).await;
    let quote = file("quote.pdf", "application/pdf");
    db.save_object(&owner, &w, &c, &quote, false).await.unwrap();
    assert_eq!(queued(&db, &w, &quote).await, Some(("pending".into(), 0)));
    // Audio is transcribed instead, whether or not transcription is on.
    let voice = file("voice-message.webm", "audio/webm");
    db.save_object(&owner, &w, &c, &voice, true).await.unwrap();
    assert_eq!(queued(&db, &w, &voice).await, None);

    let mut uploads = vec![quote];
    for index in 0..5 {
        let upload = file(&format!("sheet-{index}.csv"), "text/plain");
        db.save_object(&owner, &w, &c, &upload, false)
            .await
            .unwrap();
        uploads.push(upload);
    }
    // Two controllers race; each file is taken exactly once.
    let (a, b) = tokio::join!(db.claim_readings(4), db.claim_readings(4));
    let mut taken: Vec<String> = a
        .unwrap()
        .into_iter()
        .chain(b.unwrap())
        .map(|c| c.object)
        .collect();
    taken.sort();
    let mut expected: Vec<String> = uploads.iter().map(|u| u.id.clone()).collect();
    expected.sort();
    assert_eq!(taken, expected);
    assert!(db.claim_readings(4).await.unwrap().is_empty(), "all leased");
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn readings_are_saved_once_retried_and_given_up_for_good() {
    let db = fixture().await;
    let owner = user(&db, "reading-save-owner").await;
    let w = workspace(&db, &owner).await;
    let c = chat(&db, &owner, &w, vec![], vec![]).await;
    let invoice = file("invoice.pdf", "application/pdf");
    db.save_object(&owner, &w, &c, &invoice, false)
        .await
        .unwrap();
    let sent = sharing("Here is the invoice", None, &[&invoice]);
    db.chat_send(&owner, &w, &c, &sent).await.unwrap();
    let shown = seen(&db, &owner, &w, &c, &sent.id).await.unwrap();
    assert_eq!(shown["reading"], "pending");
    let cursor = db.sync_page(&owner, &w, None).await.unwrap()["cursor"].as_i64();

    let held = take(&db).await;
    let read = || Outcome::Read {
        kind: "pdf",
        text: "  INVOICE 42. Total due 48,000.  ".into(),
        truncated: false,
    };
    // A controller whose lease was lost can't save over the new holder.
    let stale = Claimed {
        claim: uuid::Uuid::new_v4().to_string(),
        ..held.clone()
    };
    assert!(!db.finish_reading(&stale, read()).await.unwrap());
    assert!(db.finish_reading(&held, read()).await.unwrap());
    assert!(
        !db.finish_reading(&held, read()).await.unwrap(),
        "saved once"
    );
    let row = query(
        &db.db,
        "SELECT kind,text,truncated FROM relay_object_contents WHERE workspace=$1 AND object=$2",
        vec![w.clone().into(), invoice.id.clone().into()],
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(row.try_get::<String>("", "kind").unwrap(), "pdf");
    assert_eq!(
        row.try_get::<String>("", "text").unwrap(),
        "INVOICE 42. Total due 48,000."
    );
    // People see that agents can read it; the text itself stays with agents.
    let shown = seen(&db, &owner, &w, &c, &sent.id).await.unwrap();
    assert_eq!(shown["reading"], "ready");
    assert!(shown.get("text").is_none());
    // Open conversations hear about it through the change stream.
    let page = db.sync_page(&owner, &w, cursor).await.unwrap();
    let event = page["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["kind"] == "message")
        .cloned()
        .expect("the message is announced again");
    assert_eq!(event["message"]["attachments"][0]["reading"], "ready");

    // Storage that can't be reached is tried again, after a rest, three times in all.
    let missing = file("missing.txt", "text/plain");
    db.save_object(&owner, &w, &c, &missing, false)
        .await
        .unwrap();
    for attempt in 1..=3 {
        let held = take(&db).await;
        assert!(db.finish_reading(&held, Outcome::Retry).await.unwrap());
        assert!(db.claim_readings(1).await.unwrap().is_empty(), "resting");
        let status = if attempt < 3 { "pending" } else { "failed" };
        assert_eq!(
            queued(&db, &w, &missing).await,
            Some((status.into(), attempt))
        );
        lease_ends(&db, &w).await;
    }
    assert!(db.claim_readings(1).await.unwrap().is_empty());

    // A format no reader handles is never tried again.
    let archive = file("photos.zip", "application/octet-stream");
    db.save_object(&owner, &w, &c, &archive, false)
        .await
        .unwrap();
    let held = take(&db).await;
    let outcome = Outcome::Unreadable { kind: None };
    assert!(db.finish_reading(&held, outcome).await.unwrap());
    assert_eq!(
        queued(&db, &w, &archive).await,
        Some(("unsupported".into(), 1))
    );
    lease_ends(&db, &w).await;
    assert!(db.claim_readings(1).await.unwrap().is_empty());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_lost_controller_gives_its_file_up_after_the_last_try() {
    let db = fixture().await;
    let owner = user(&db, "reading-lost-owner").await;
    let w = workspace(&db, &owner).await;
    let c = chat(&db, &owner, &w, vec![], vec![]).await;
    let notes = file("notes.txt", "text/plain");
    db.save_object(&owner, &w, &c, &notes, false).await.unwrap();
    for _ in 0..3 {
        take(&db).await;
        lease_ends(&db, &w).await;
    }
    // The third lease ran out too: the next look gives the file up.
    assert!(db.claim_readings(1).await.unwrap().is_empty());
    assert_eq!(queued(&db, &w, &notes).await, Some(("failed".into(), 3)));
    assert_eq!(db.next_reading_in().await.unwrap(), None);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn only_people_who_can_read_the_message_see_its_reading() {
    let db = fixture().await;
    let owner = user(&db, "reading-people-owner").await;
    let outsider = user(&db, "reading-people-outsider").await;
    let w = workspace(&db, &owner).await;
    super::super::super::tests::join(&db, &owner, &w, &outsider, "Member").await;
    let c = chat(&db, &owner, &w, vec![], vec![]).await;
    let plan = file("plan.docx", "text/plain");
    db.save_object(&owner, &w, &c, &plan, false).await.unwrap();
    let sent = sharing("Plan attached", None, &[&plan]);
    db.chat_send(&owner, &w, &c, &sent).await.unwrap();
    read_as(&db, "word", "The plan.").await;
    assert_eq!(
        seen(&db, &owner, &w, &c, &sent.id).await.unwrap()["reading"],
        "ready"
    );
    assert!(seen(&db, &outsider, &w, &c, &sent.id).await.is_err());
    // Audio shows its transcript status instead.
    let voice = file("voice-message.webm", "audio/webm");
    db.save_object(&owner, &w, &c, &voice, false).await.unwrap();
    let spoken = sharing("Shared files", None, &[&voice]);
    db.chat_send(&owner, &w, &c, &spoken).await.unwrap();
    let shown = seen(&db, &owner, &w, &c, &spoken.id).await.unwrap();
    assert_eq!(shown.get("reading"), None);
    assert_eq!(shown["transcript_status"], json!("not_enabled"));
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn schema_100_reads_files_already_shared_in_messages() {
    let db = fixture().await;
    let owner = user(&db, "reading-backfill-owner").await;
    let w = workspace(&db, &owner).await;
    let c = chat(&db, &owner, &w, vec![], vec![]).await;
    // Before schema 100, Chat recorded documents as opaque bytes.
    let shared = file("contract.pdf", "application/octet-stream");
    let unsent = file("draft.txt", "text/plain");
    let voice = file("voice-message.webm", "audio/webm");
    for upload in [&shared, &unsent, &voice] {
        db.save_object(&owner, &w, &c, upload, false).await.unwrap();
    }
    db.chat_send(&owner, &w, &c, &sharing("Signed", None, &[&shared, &voice]))
        .await
        .unwrap();
    execute(
        &db.db,
        "DELETE FROM relay_object_contents WHERE workspace=$1",
        vec![w.clone().into()],
    )
    .await
    .unwrap();
    let backfill = include_str!("object_contents.sql")
        .split_once("INSERT INTO relay_object_contents")
        .unwrap()
        .1;
    db.db
        .execute_unprepared(&format!("INSERT INTO relay_object_contents{backfill}"))
        .await
        .unwrap();
    assert_eq!(queued(&db, &w, &shared).await, Some(("pending".into(), 0)));
    assert_eq!(queued(&db, &w, &unsent).await, None, "never shared");
    assert_eq!(queued(&db, &w, &voice).await, None, "audio is transcribed");
}

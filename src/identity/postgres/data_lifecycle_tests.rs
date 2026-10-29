//! Cleanup removes duplicates and delivery notices but never what a reader sees.
use super::{reply_output_tests::begin, *};
use crate::identity::worker_control::model::Completion;

async fn chunks(db: &PgStore, reply: &str) -> i64 {
    query(&db.db,"SELECT count(*) AS n FROM relay_assignment_output o JOIN relay_worker_assignments a ON a.id=o.assignment WHERE a.reply=$1",vec![reply.into()])
        .await.unwrap().unwrap().try_get("", "n").unwrap()
}
/// Pretend the reply stopped long ago, past the settle window.
async fn age(db: &PgStore, reply: &str) {
    execute(
        &db.db,
        "UPDATE relay_worker_assignments SET finished_ms=finished_ms-3600000 WHERE reply=$1",
        vec![reply.into()],
    )
    .await
    .unwrap();
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_stopped_reply_keeps_its_partial_text_after_its_chunks_are_removed() {
    let (db, owner, w, chat, hash, mut update) = begin().await;
    update.phase = "responding".into();
    update.text = "First saved part 🦀\n".into();
    db.append_output(&update, &hash).await.unwrap();
    update.sequence = 2;
    update.text = "Second saved part".into();
    db.append_output(&update, &hash).await.unwrap();
    let id = update.assignment.scope.reply.clone();
    // A running reply is never compacted.
    assert_eq!(db.compact_replies(600, 500).await.unwrap(), 0);
    db.cancel_assignment(&owner, &w, &chat, &id).await.unwrap();
    let before = db
        .response_body(&owner, &w, &chat, &id, false)
        .await
        .unwrap();
    assert_eq!(
        db.compact_replies(600, 500).await.unwrap(),
        0,
        "within the settle window"
    );
    age(&db, &id).await;
    assert_eq!(db.compact_replies(600, 500).await.unwrap(), 1);
    assert_eq!(chunks(&db, &id).await, 0);
    let after = db
        .response_body(&owner, &w, &chat, &id, false)
        .await
        .unwrap();
    assert_eq!(after["body"], "First saved part 🦀\nSecond saved part");
    assert_eq!(after, before, "the reader sees exactly the same reply");
    assert_eq!(db.compact_replies(600, 500).await.unwrap(), 0);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_completed_reply_drops_its_chunks_and_keeps_its_final_text() {
    let (db, owner, w, chat, hash, mut update) = begin().await;
    update.phase = "responding".into();
    update.text = "Streaming draft".into();
    db.append_output(&update, &hash).await.unwrap();
    let id = update.assignment.scope.reply.clone();
    db.complete_controlled(
        &Completion {
            assignment: update.assignment.clone(),
            state: "complete".into(),
            text: "The final answer".into(),
        },
        &hash,
    )
    .await
    .unwrap();
    age(&db, &id).await;
    assert_eq!(db.compact_replies(600, 500).await.unwrap(), 1);
    assert_eq!(chunks(&db, &id).await, 0);
    let body = db
        .response_body(&owner, &w, &chat, &id, false)
        .await
        .unwrap();
    assert_eq!(body["body"], "The final answer");
    assert!(
        query(
            &db.db,
            "SELECT reply FROM relay_reply_partials WHERE reply=$1",
            vec![id.into()]
        )
        .await
        .unwrap()
        .is_none(),
        "a completed reply needs no saved partial text"
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn old_live_update_positions_reload_instead_of_missing_removed_notices() {
    let (db, owner, w, _, _, _) = begin().await;
    let page = db.sync_page(&owner, &w, Some(0)).await.unwrap();
    assert!(page.get("reset").is_none() && !page["events"].as_array().unwrap().is_empty());
    let high = page["cursor"].as_i64().unwrap();
    assert_eq!(
        db.expire_sync_events(30, 500).await.unwrap(),
        0,
        "recent notices stay"
    );
    execute(
        &db.db,
        "UPDATE relay_sync_events SET created_ms=0 WHERE workspace=$1",
        vec![w.clone().into()],
    )
    .await
    .unwrap();
    assert!(db.expire_sync_events(30, 500).await.unwrap() > 0);
    // A browser from before the removal reloads; one at the floor continues.
    assert_eq!(
        db.sync_page(&owner, &w, Some(0)).await.unwrap()["reset"],
        true
    );
    let current = db.sync_page(&owner, &w, Some(high)).await.unwrap();
    assert!(current.get("reset").is_none());
}

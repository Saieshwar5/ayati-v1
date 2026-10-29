//! Only authors change their messages; deleting stops the work a message asked
//! for; reactions are a fixed, harmless set that never starts work.
use super::{tests::*, *};

async fn latest(db: &PgStore, user: &User, w: &str, chat: &str, id: &str) -> Value {
    db.chat_messages(user, w, chat).await.unwrap()["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == id)
        .cloned()
        .unwrap()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn authors_edit_and_delete_their_own_messages_only() {
    let db = fixture().await;
    let owner = user(&db, "edit-owner").await;
    let w = workspace(&db, &owner).await;
    let member = user(&db, "edit-member").await;
    join(&db, &owner, &w, &member, "User").await;
    let c = chat(&db, &owner, &w, vec![member.id.clone()], vec![]).await;
    let sent = message("First draft", None);
    db.chat_send(&member, &w, &c, &sent).await.unwrap();
    // Not even the workspace owner may rewrite someone else's words.
    assert!(
        db.edit_message(&owner, &w, &c, &sent.id, "Changed")
            .await
            .is_err()
    );
    assert!(db.delete_message(&owner, &w, &c, &sent.id).await.is_err());
    assert!(
        db.edit_message(&member, &w, &c, &sent.id, "  ")
            .await
            .is_err()
    );
    db.edit_message(&member, &w, &c, &sent.id, "Second draft")
        .await
        .unwrap();
    let edited = latest(&db, &owner, &w, &c, &sent.id).await;
    assert_eq!(edited["content"], "Second draft");
    assert!(edited["edited_at"].is_i64());
    db.delete_message(&member, &w, &c, &sent.id).await.unwrap();
    let deleted = latest(&db, &owner, &w, &c, &sent.id).await;
    assert_eq!(
        (deleted["state"].as_str(), deleted["content"].as_str()),
        (Some("deleted"), Some(""))
    );
    // A deleted message cannot be edited, deleted again or reacted to.
    assert!(
        db.edit_message(&member, &w, &c, &sent.id, "Back")
            .await
            .is_err()
    );
    assert!(db.delete_message(&member, &w, &c, &sent.id).await.is_err());
    assert!(
        db.react(&owner, &w, &c, &sent.id, "👍", true)
            .await
            .is_err()
    );
    // It no longer counts as unread for the other person.
    let unread = db.unread(&owner, &w).await.unwrap();
    assert!(
        unread["unread"]
            .as_array()
            .unwrap()
            .iter()
            .all(|u| u["chat"] != c)
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn deleting_a_request_stops_the_agent_work_it_started() {
    let db = fixture().await;
    let owner = user(&db, "delete-request").await;
    let w = workspace(&db, &owner).await;
    let (c, agent) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    let ask = message("Please check the stock", Some(&agent));
    let job = db
        .chat_send(&owner, &w, &c, &ask)
        .await
        .unwrap()
        .expect("agent work queued");
    let reply = latest(&db, &owner, &w, &c, &job.reply).await;
    assert_eq!(reply["state"], "queued");
    assert!(
        reply.get("request_message").is_none(),
        "the link stays server-side"
    );
    // Editing keeps the work running with the original request.
    db.edit_message(&owner, &w, &c, &ask.id, "Please check the stock today")
        .await
        .unwrap();
    assert_eq!(
        latest(&db, &owner, &w, &c, &job.reply).await["state"],
        "queued"
    );
    assert_eq!(db.delete_message(&owner, &w, &c, &ask.id).await.unwrap(), 1);
    assert_eq!(
        latest(&db, &owner, &w, &c, &job.reply).await["state"],
        "cancelled"
    );
    let finished = query(
        &db.db,
        "SELECT state FROM relay_chat_jobs WHERE reply=$1",
        vec![job.reply.clone().into()],
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(finished.try_get::<String>("", "state").unwrap(), "finished");
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn reactions_are_per_person_idempotent_and_never_start_work() {
    let db = fixture().await;
    let owner = user(&db, "react-owner").await;
    let w = workspace(&db, &owner).await;
    let member = user(&db, "react-member").await;
    let outsider = user(&db, "react-outsider").await;
    join(&db, &owner, &w, &member, "User").await;
    let (c, agent) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    db.channel_members(&owner, &w, &c, &[owner.id.clone(), member.id.clone()])
        .await
        .unwrap();
    let note = message("Delivery is booked", None);
    db.chat_send(&owner, &w, &c, &note).await.unwrap();
    let jobs = || async {
        query(
            &db.db,
            "SELECT count(*) AS n FROM relay_chat_jobs WHERE workspace=$1",
            vec![w.clone().into()],
        )
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "n")
        .unwrap()
    };
    let before = jobs().await;
    for _ in 0..2 {
        db.react(&member, &w, &c, &note.id, "👍", true)
            .await
            .unwrap();
    }
    db.react(&owner, &w, &c, &note.id, "👍", true)
        .await
        .unwrap();
    db.react(&owner, &w, &c, &note.id, "✅", true)
        .await
        .unwrap();
    let mine = latest(&db, &member, &w, &c, &note.id).await;
    let mut counts: Vec<_> = mine["reactions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r["emoji"].as_str().unwrap().to_owned(),
                r["count"].as_i64().unwrap(),
                r["mine"].as_bool().unwrap(),
            )
        })
        .collect();
    counts.sort();
    assert_eq!(
        counts,
        vec![("✅".into(), 1, false), ("👍".into(), 2, true)]
    );
    db.react(&member, &w, &c, &note.id, "👍", false)
        .await
        .unwrap();
    db.react(&member, &w, &c, &note.id, "👍", false)
        .await
        .unwrap();
    assert_eq!(
        latest(&db, &member, &w, &c, &note.id).await["reactions"][0]["count"],
        1
    );
    assert!(
        db.react(&member, &w, &c, &note.id, "<script>", true)
            .await
            .is_err()
    );
    assert!(
        db.react(&outsider, &w, &c, &note.id, "👍", true)
            .await
            .is_err()
    );
    assert_eq!(jobs().await, before, "a reaction is never a request");
    let _ = agent;
    // Reactions reach open screens as a change to that one message.
    let cursor = db.sync_page(&member, &w, None).await.unwrap()["cursor"]
        .as_i64()
        .unwrap();
    db.react(&owner, &w, &c, &note.id, "🎉", true)
        .await
        .unwrap();
    let page = db.sync_page(&member, &w, Some(cursor)).await.unwrap();
    let changed = page["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["kind"] == "message" && e["message"]["id"] == note.id)
        .unwrap()
        .clone();
    assert!(
        changed["message"]["reactions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["emoji"] == "🎉")
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn author_edits_and_deletes_keep_answers_already_given() {
    let db = fixture().await;
    let owner = user(&db, "keep-answers").await;
    let w = workspace(&db, &owner).await;
    let c = chat(&db, &owner, &w, vec![], vec![]).await;
    let first = message("Typo in this mesage", None);
    let second = message("Something to remove", None);
    db.chat_send(&owner, &w, &c, &first).await.unwrap();
    db.chat_send(&owner, &w, &c, &second).await.unwrap();
    let generation = || async {
        query(
            &db.db,
            "SELECT COALESCE(sum(revision),0)::bigint AS n FROM relay_context_epochs WHERE workspace=$1",
            vec![w.clone().into()],
        )
        .await
        .unwrap()
        .map(|r| r.try_get::<i64>("", "n").unwrap())
        .unwrap_or(0)
    };
    let before = generation().await;
    db.edit_message(&owner, &w, &c, &first.id, "Typo in this message")
        .await
        .unwrap();
    db.delete_message(&owner, &w, &c, &second.id).await.unwrap();
    // Earlier agent answers in this conversation are not withdrawn.
    assert_eq!(generation().await, before);
}

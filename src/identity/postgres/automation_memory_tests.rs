//! Automation memory (schema 115): event triggers run in private work rooms like
//! schedules, and each automation chooses what a run sees of earlier runs.
use super::{event_tests::setup, *};

async fn messages(db: &PgStore, chat: &str) -> String {
    json_rows(
        &db.db,
        "SELECT jsonb_build_object('content',content) AS data FROM relay_chat_messages WHERE chat=$1 ORDER BY sequence",
        vec![chat.into()],
    )
    .await
    .unwrap()
    .iter()
    .map(|m| m["content"].as_str().unwrap_or_default().to_owned())
    .collect::<Vec<_>>()
    .join("\n")
}

/// The newest queued event job, claimed as a worker would.
async fn claim_latest(db: &PgStore) -> crate::identity::chat::model::Job {
    let claim = uuid::Uuid::new_v4().to_string();
    let row = query(&db.db, "UPDATE relay_chat_jobs SET state='running',claim=$1,attempts=1,lease_until=floor(extract(epoch FROM clock_timestamp()))::bigint+300
        WHERE reply=(SELECT j.reply FROM relay_chat_jobs j JOIN relay_event_deliveries d ON d.workspace=j.workspace AND d.reply=j.reply WHERE j.state='queued' ORDER BY j.sequence DESC LIMIT 1) RETURNING payload",
        vec![claim.clone().into()]).await.unwrap().unwrap();
    let mut job: crate::identity::chat::model::Job =
        serde_json::from_value(row.try_get("", "payload").unwrap()).unwrap();
    job.claim = Some(claim);
    job
}

async fn is_work_room(db: &PgStore, chat: &str) -> bool {
    query(
        &db.db,
        "SELECT 1 AS found FROM relay_chats WHERE id=$1 AND work",
        vec![chat.into()],
    )
    .await
    .unwrap()
    .is_some()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_fresh_trigger_gives_every_run_its_own_room_and_needs_no_conversation() {
    let mut s = setup().await;
    s.input.chat = None;
    s.save().await;
    let before = messages(&s.db, &s.c).await;

    s.accept("first", "delayed").await;
    assert_eq!(s.db.dispatch_events().await.unwrap(), 1);
    let first = claim_latest(&s.db).await;
    assert!(is_work_room(&s.db, &first.chat).await);
    s.db.chat_finish(&first, "FIRST-RUN-RESULT", "complete")
        .await
        .unwrap();

    s.accept("second", "delayed").await;
    assert_eq!(s.db.dispatch_events().await.unwrap(), 1);
    let second = claim_latest(&s.db).await;
    assert!(is_work_room(&s.db, &second.chat).await);
    assert_ne!(first.chat, second.chat, "fresh runs never share a room");
    assert!(
        !messages(&s.db, &second.chat)
            .await
            .contains("FIRST-RUN-RESULT"),
        "a fresh run sees nothing from an earlier run"
    );
    assert_eq!(
        messages(&s.db, &s.c).await,
        before,
        "with no conversation chosen, nothing is posted anywhere"
    );
    let details =
        s.db.event_trigger_details(
            &s.owner,
            &s.w,
            &s.a,
            &s.input.id,
            std::slice::from_ref(&s.source),
        )
        .await
        .unwrap();
    assert_eq!(details["memory"], "fresh");
    assert!(details["chat"].is_null());
    assert!(details.get("room").is_none(), "the room is never shown");
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_remembering_trigger_keeps_one_room_and_posts_a_short_result() {
    let mut s = setup().await;
    s.input.memory = Some("remember".into());
    s.save().await;

    s.accept("first", "delayed").await;
    assert_eq!(s.db.dispatch_events().await.unwrap(), 1);
    let first = claim_latest(&s.db).await;
    assert_ne!(first.chat, s.c, "the run happens in a work room");
    s.db.chat_finish(&first, "FIRST-RUN-RESULT", "complete")
        .await
        .unwrap();
    let posted = messages(&s.db, &s.c).await;
    assert!(posted.contains("**Review order change**"));
    assert!(posted.contains("FIRST-RUN-RESULT"));
    assert!(posted.contains(&format!("[View run](run:{})", first.reply)));

    s.accept("second", "delayed").await;
    assert_eq!(s.db.dispatch_events().await.unwrap(), 1);
    let second = claim_latest(&s.db).await;
    assert_eq!(first.chat, second.chat, "remembering runs share one room");
    assert!(
        messages(&s.db, &second.chat)
            .await
            .contains("FIRST-RUN-RESULT"),
        "a remembering run sees the earlier run"
    );
    let activity =
        s.db.activity_request(&s.owner, &s.w, &first.reply)
            .await
            .unwrap();
    assert_eq!(activity["trigger"]["memory"], "remember");
    assert_eq!(activity["trigger"]["posted"], "posted");
    assert_eq!(activity["trigger"]["room"], true);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn an_unknown_memory_choice_is_refused() {
    let mut s = setup().await;
    s.input.memory = Some("forever".into());
    assert!(
        s.db.save_event_trigger(&s.owner, &s.w, &s.a, &s.input, Some(&s.source))
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_fresh_schedule_opens_a_new_room_for_each_run() {
    let (db, owner, w, c, a) = super::automation_tests::setup().await;
    let mut input = super::automation_tests::schedule(&c, Some(3600));
    input.chat = None;
    input.memory = Some("fresh".into());
    db.save_schedule(&owner, &w, &a, &input).await.unwrap();
    assert_eq!(db.dispatch_schedules_at(input.first_ms).await.unwrap(), 1);
    let claim = |db: &PgStore| {
        let db = db.clone();
        async move {
            let row = query(&db.db, "UPDATE relay_chat_jobs SET state='running',claim=$1,attempts=1,lease_until=floor(extract(epoch FROM clock_timestamp()))::bigint+300
                WHERE reply=(SELECT reply FROM relay_chat_jobs WHERE state='queued' ORDER BY sequence LIMIT 1) RETURNING payload",
                vec![uuid::Uuid::new_v4().to_string().into()]).await.unwrap().unwrap();
            serde_json::from_value::<crate::identity::chat::model::Job>(
                row.try_get("", "payload").unwrap(),
            )
            .unwrap()
        }
    };
    let first = claim(&db).await;
    execute(
        &db.db,
        "UPDATE relay_chat_jobs SET state='finished' WHERE reply=$1",
        vec![first.reply.clone().into()],
    )
    .await
    .unwrap();
    assert_eq!(
        db.dispatch_schedules_at(input.first_ms + 3_600_000)
            .await
            .unwrap(),
        1
    );
    let second = claim(&db).await;
    assert!(is_work_room(&db, &first.chat).await && is_work_room(&db, &second.chat).await);
    assert_ne!(first.chat, second.chat);
    let saved: Option<String> = query(
        &db.db,
        "SELECT room FROM relay_agent_schedules WHERE id=$1",
        vec![input.id.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "room")
    .unwrap();
    assert!(saved.is_none(), "a fresh schedule keeps no lasting room");
}

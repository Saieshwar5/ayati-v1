//! Work rooms: a schedule runs in its own room, never in a conversation; it may
//! post one short result message where its person chose.
use super::super::{
    automation_tests::{schedule, setup},
    tests::*,
    *,
};
use serde_json::json;

/// Scheduled runs wait for the worker pool; a test takes the oldest one directly.
async fn claim(db: &PgStore) -> crate::identity::chat::model::Job {
    let claim = uuid::Uuid::new_v4().to_string();
    let row = query(&db.db, "UPDATE relay_chat_jobs SET state='running',claim=$1,attempts=1,lease_until=floor(extract(epoch FROM clock_timestamp()))::bigint+300
        WHERE reply=(SELECT reply FROM relay_chat_jobs WHERE state='queued' ORDER BY sequence LIMIT 1) RETURNING payload", vec![claim.clone().into()]).await.unwrap().unwrap();
    let mut job: crate::identity::chat::model::Job =
        serde_json::from_value(row.try_get("", "payload").unwrap()).unwrap();
    job.claim = Some(claim);
    job
}
async fn messages(db: &PgStore, w: &str, chat: &str) -> Vec<Value> {
    json_rows(&db.db, "SELECT jsonb_build_object('author',author,'kind',kind,'content',content) AS data FROM relay_chat_messages WHERE workspace=$1 AND chat=$2 ORDER BY sequence",
        vec![w.into(), chat.into()]).await.unwrap()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_schedule_runs_in_its_own_room_and_never_in_a_conversation() {
    let (db, owner, w, c, a) = setup().await;
    let mut input = schedule(&c, Some(3600));
    input.chat = None;
    db.save_schedule(&owner, &w, &a, &input).await.unwrap();
    let before = messages(&db, &w, &c).await;
    assert_eq!(db.dispatch_schedules_at(input.first_ms).await.unwrap(), 1);
    let room = query(
        &db.db,
        "SELECT room FROM relay_agent_schedules WHERE id=$1",
        vec![input.id.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get::<String>("", "room")
    .unwrap();
    let shape = json_rows(&db.db, "SELECT jsonb_build_object('kind',c.kind,'work',c.work,'visibility',c.visibility,'people',(SELECT jsonb_agg(user_id) FROM relay_chat_people p WHERE p.workspace=c.workspace AND p.chat=c.id),'agents',(SELECT jsonb_agg(agent) FROM relay_chat_agents x WHERE x.workspace=c.workspace AND x.chat=c.id)) AS data FROM relay_chats c WHERE c.id=$1",
        vec![room.clone().into()]).await.unwrap().remove(0);
    assert_eq!(
        shape,
        json!({"kind":"direct","work":true,"visibility":"private","people":[owner.id],"agents":[a]})
    );
    assert_eq!(
        messages(&db, &w, &c).await,
        before,
        "the conversation gets nothing"
    );
    let listed = db.chats(&owner, &w).await.unwrap();
    assert!(
        !listed["chats"]
            .as_array()
            .unwrap()
            .iter()
            .any(|chat| chat["id"] == room),
        "a work room never appears in Chat"
    );
    assert!(
        db.chat_send(&owner, &w, &room, &message("Hello", Some(&a)))
            .await
            .is_err(),
        "people don't write into a work room"
    );

    let job = claim(&db).await;
    assert_eq!(job.chat, room);
    db.chat_finish(&job, "Two deliveries are late.", "complete")
        .await
        .unwrap();
    assert_eq!(
        messages(&db, &w, &c).await,
        before,
        "Activity only: nothing is posted"
    );
    let details = db.activity_request(&owner, &w, &job.reply).await.unwrap();
    assert_eq!(
        (
            details["schedule"]["name"].as_str(),
            details["schedule"]["post_to"].is_null()
        ),
        (Some("Delivery review"), true)
    );

    // The next run uses the same room: its memory of earlier runs.
    assert_eq!(
        db.dispatch_schedules_at(input.first_ms + 3_600_000)
            .await
            .unwrap(),
        1
    );
    assert_eq!(claim(&db).await.chat, room);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_schedule_posts_one_short_result_where_its_person_chose_while_they_still_may() {
    let (db, owner, w, c, a) = setup().await;
    let input = schedule(&c, Some(3600));
    db.save_schedule(&owner, &w, &a, &input).await.unwrap();
    assert_eq!(db.dispatch_schedules_at(input.first_ms).await.unwrap(), 1);
    let job = claim(&db).await;
    assert_ne!(job.chat, c, "the run itself is not in the conversation");
    let long = "Late deliveries: ".to_string() + &"order ".repeat(120);
    db.chat_finish(&job, &long, "complete").await.unwrap();
    let posted = messages(&db, &w, &c).await;
    let last = posted.last().unwrap();
    assert_eq!(
        (last["author"].as_str(), last["kind"].as_str()),
        (Some(a.as_str()), Some("agent"))
    );
    let text = last["content"].as_str().unwrap();
    assert!(
        text.starts_with("**Delivery review**")
            && text.contains('…')
            && text.ends_with(&format!("[View run](run:{})", job.reply))
    );
    assert!(
        text.chars().count() < 500,
        "one short message, not the whole result"
    );
    let details = db.activity_request(&owner, &w, &job.reply).await.unwrap();
    assert_eq!(
        (
            details["schedule"]["posted"].as_str(),
            details["schedule"]["post_to"].as_str()
        ),
        (Some("posted"), Some("Operations"))
    );

    // The agent leaves the conversation while a run is working: the run
    // finishes, its post is skipped and Activity says why.
    assert_eq!(
        db.dispatch_schedules_at(input.first_ms + 3_600_000)
            .await
            .unwrap(),
        1
    );
    let next = claim(&db).await;
    execute(
        &db.db,
        "DELETE FROM relay_chat_agents WHERE workspace=$1 AND chat=$2 AND agent=$3",
        vec![w.clone().into(), c.clone().into(), a.clone().into()],
    )
    .await
    .unwrap();
    let count = messages(&db, &w, &c).await.len();
    db.chat_finish(&next, "Nothing late today.", "complete")
        .await
        .unwrap();
    assert_eq!(messages(&db, &w, &c).await.len(), count);
    let details = db.activity_request(&owner, &w, &next.reply).await.unwrap();
    assert_eq!(details["schedule"]["posted"], "skipped");
    assert!(
        details["schedule"]["post_reason"]
            .as_str()
            .unwrap()
            .contains("can no longer post")
    );
    // And the schedule stops for review rather than quietly running without its post.
    assert_eq!(
        db.dispatch_schedules_at(input.first_ms + 7_200_000)
            .await
            .unwrap(),
        0
    );
    let stopped = query(
        &db.db,
        "SELECT state FROM relay_agent_schedules WHERE id=$1",
        vec![input.id.clone().into()],
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(stopped.try_get::<String>("", "state").unwrap(), "blocked");
}

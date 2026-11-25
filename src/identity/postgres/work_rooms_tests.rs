//! Work rooms: a task that runs on its own works in its own room, never in a
//! conversation; it may post one short result message where it was asked for.
use super::super::{automation_tests::setup, tests::*, *};
use serde_json::json;

/// Task runs wait for the worker pool; a test takes the oldest one directly.
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

/// A repeating task of `owner` with agent `a`, working in a new work room and
/// posting to `post_to` if given, with a run due now. Returns the task and room.
async fn task_in_room(
    db: &PgStore,
    w: &str,
    owner: &User,
    a: &str,
    post_to: Option<&str>,
) -> (String, String) {
    let tx = db.db.begin().await.unwrap();
    let room = super::open(&tx, w, "Delivery review", "Runs of a task", &owner.id, a)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let task = uuid::Uuid::new_v4().to_string();
    execute(&db.db, "INSERT INTO relay_tasks(workspace,id,agent,kind,origin,title,requester,chat,room,repeat,anchor_ms,instructions) VALUES($1,$2,$3,'commitment','person','Delivery review',$4,$5,$6,'{\"unit\":\"hour\"}',1,'Review delivery exceptions.')",
        vec![w.into(), task.clone().into(), a.into(), owner.id.clone().into(), post_to.unwrap_or(&room).into(), post_to.map(|_| room.clone()).into()]).await.unwrap();
    due(db, &task).await;
    (task, room)
}
async fn due(db: &PgStore, task: &str) {
    execute(
        &db.db,
        "UPDATE relay_task_wakeups SET state='cancelled' WHERE task=$1 AND state='pending'",
        vec![task.into()],
    )
    .await
    .unwrap();
    execute(&db.db, "INSERT INTO relay_task_wakeups(workspace,id,task,due_ms,kind) SELECT workspace,$2,id,(extract(epoch FROM clock_timestamp())*1000)::bigint-1000,'repeat' FROM relay_tasks WHERE id=$1",
        vec![task.into(), uuid::Uuid::new_v4().to_string().into()]).await.unwrap();
}
async fn post(db: &PgStore, reply: &str) -> Value {
    json_rows(&db.db, "SELECT jsonb_build_object('state',state,'reason',reason) AS data FROM relay_run_posts WHERE reply=$1", vec![reply.into()])
        .await.unwrap().pop().unwrap_or(Value::Null)
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_task_runs_in_its_own_room_and_never_in_a_conversation() {
    let (db, owner, w, c, a) = setup().await;
    let before = messages(&db, &w, &c).await;
    let (task, room) = task_in_room(&db, &w, &owner, &a, None).await;
    assert_eq!(db.dispatch_task_wakeups().await.unwrap(), 1);
    let shape = json_rows(&db.db, "SELECT jsonb_build_object('kind',c.kind,'work',c.work,'visibility',c.visibility,'people',(SELECT jsonb_agg(user_id) FROM relay_chat_people p WHERE p.workspace=c.workspace AND p.chat=c.id),'agents',(SELECT jsonb_agg(agent) FROM relay_chat_agents x WHERE x.workspace=c.workspace AND x.chat=c.id)) AS data FROM relay_chats c WHERE c.id=$1",
        vec![room.clone().into()]).await.unwrap().remove(0);
    assert_eq!(
        shape,
        json!({"kind":"direct","work":true,"visibility":"private","people":[owner.id],"agents":[a]})
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
    assert!(
        job.prompt
            .contains("What to do each run: Review delivery exceptions.")
    );
    db.chat_finish(&job, "Two deliveries are late.", "complete")
        .await
        .unwrap();
    assert_eq!(
        messages(&db, &w, &c).await,
        before,
        "Activity only: nothing is posted"
    );
    assert!(post(&db, &job.reply).await.is_null());

    // The next run uses the same room: its memory of earlier runs.
    due(&db, &task).await;
    assert_eq!(db.dispatch_task_wakeups().await.unwrap(), 1);
    assert_eq!(claim(&db).await.chat, room);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_task_posts_one_short_result_where_it_was_asked_for_while_its_person_still_may() {
    let (db, owner, w, c, a) = setup().await;
    let (task, room) = task_in_room(&db, &w, &owner, &a, Some(&c)).await;
    assert_eq!(db.dispatch_task_wakeups().await.unwrap(), 1);
    let job = claim(&db).await;
    assert_eq!(job.chat, room, "the run itself is not in the conversation");
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
    assert_eq!(post(&db, &job.reply).await["state"], "posted");

    // The agent leaves the conversation while a run is working: the run
    // finishes, its post is skipped and the reason is kept.
    due(&db, &task).await;
    assert_eq!(db.dispatch_task_wakeups().await.unwrap(), 1);
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
    let skipped = post(&db, &next.reply).await;
    assert_eq!(skipped["state"], "skipped");
    assert!(
        skipped["reason"]
            .as_str()
            .unwrap()
            .contains("can no longer post")
    );
}

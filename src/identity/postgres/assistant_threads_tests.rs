//! Assistant threads (schema 128): suggested by the assistant, started by the
//! member, sent as the member, answered on their own while running, private
//! about who wrote what, and stopped, resumed and deleted by the member.
use super::super::super::{tests::*, *};
use super::{LIMIT, NewThread};
use crate::identity::{
    agents::config::{SaveAgent, Settings},
    chat::model::Job,
};
use serde_json::json;

async fn shared_agent(db: &PgStore, owner: &User, w: &str, name: &str) -> String {
    let input = SaveAgent {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.into(),
        role: "Leave and policies".into(),
        model: "fixture".into(),
        audience: "everyone".into(),
        people: None,
        revision: 0,
        settings: Settings {
            provider: "openai".into(),
            ..Default::default()
        },
    };
    db.save_agent_profile(owner, w, &input).await.unwrap();
    input.id
}
async fn one(db: &PgStore, sql: &str, args: Vec<sea_orm::Value>) -> Value {
    query(&db.db, &format!("SELECT to_jsonb(({sql})) AS v"), args)
        .await
        .unwrap()
        .unwrap()
        .try_get("", "v")
        .unwrap()
}
async fn stored(db: &PgStore, id: &str) -> Value {
    one(
        db,
        "SELECT m FROM relay_chat_messages m WHERE id=$1",
        vec![id.into()],
    )
    .await
}
async fn thread(db: &PgStore, id: &str) -> Value {
    one(
        db,
        "SELECT t FROM relay_work_threads t WHERE id=$1",
        vec![id.into()],
    )
    .await
}
/// The queued job in `chat`, claimed as a worker would.
async fn queued(db: &PgStore, chat: &str) -> Job {
    let payload = one(
        db,
        "SELECT j.payload FROM relay_chat_jobs j WHERE j.payload->>'chat'=$1 AND j.state='queued'",
        vec![chat.into()],
    )
    .await;
    super::super::super::handoff_tests::claim(db, &serde_json::from_value(payload).unwrap()).await
}
async fn room(db: &PgStore, agent: &str) -> String {
    one(
        db,
        "SELECT chat FROM relay_chat_agents WHERE agent=$1",
        vec![agent.into()],
    )
    .await
    .as_str()
    .unwrap()
    .to_owned()
}
fn new_thread(kind: &str, to: &str) -> NewThread {
    NewThread {
        id: uuid::Uuid::new_v4().to_string(),
        kind: kind.into(),
        counterpart: to.into(),
        title: "Leave in December".into(),
        opening: "Hi, I'd like leave from 22 to 26 December.".into(),
    }
}
async fn reply(db: &PgStore, job: &Job, thread: &str, key: &str, text: &str) -> Value {
    db.thread_reply(
        job,
        &serde_json::from_value(json!({"thread":thread,"key":key,"text":text})).unwrap(),
    )
    .await
    .unwrap()
}

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn a_suggested_thread_runs_on_its_own_once_started_until_it_is_stopped() {
    let mut db = fixture().await;
    db.pooled = true;
    let ravi = user(&db, "ravi").await;
    let w = workspace(&db, &ravi).await;
    let mine = db
        .ensure_assistant(&ravi, &w, "fixture", "openai")
        .await
        .unwrap();
    let hr = shared_agent(&db, &ravi, &w, "HR Desk").await;
    let own_room = room(&db, &mine).await;

    // Ravi asks his assistant; it suggests a thread. Nothing is sent.
    db.chat_send(
        &ravi,
        &w,
        &own_room,
        &message("Get my December leave approved.", Some(&mine)),
    )
    .await
    .unwrap();
    let asked = queued(&db, &own_room).await;
    let suggested = db
        .suggest_thread(
            &asked,
            &serde_json::from_value(json!({"to":"hr desk","title":"Leave in December","opening":"Hi, I'd like leave from 22 to 26 December."})).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(suggested["state"], "suggested");
    let id = suggested["thread"].as_str().unwrap().to_owned();
    let listed = db.threads(&ravi, &w).await.unwrap();
    assert_eq!(listed["threads"][0]["state"], "suggested");
    assert_eq!(listed["threads"][0]["name"], "HR Desk");
    assert_eq!(listed["threads"][0]["origin"], "assistant");
    assert!(thread(&db, &id).await["chat"].is_null());
    // The assistant can't send before Ravi starts it.
    assert_eq!(
        reply(&db, &asked, &id, "early", "Hello").await["state"],
        "invalid"
    );
    db.chat_finish(&asked, "I suggested a thread with HR Desk.", "complete")
        .await
        .unwrap();

    // Ravi starts it: the opening is his message to HR Desk, which is asked.
    let (chat, job) = db.thread_start(&ravi, &w, &id).await.unwrap();
    let hr_job = job.expect("HR Desk is asked");
    assert_eq!(hr_job.agent, hr);
    let opening = stored(&db, &id).await;
    assert_eq!(opening["author"], ravi.id.as_str());
    assert_eq!(opening["kind"], "user");
    assert_eq!(opening["chat"], chat.as_str());
    assert_eq!(opening["assisted"]["by"], "assistant");
    assert_eq!(opening["assisted"]["thread"], id.as_str());
    // Starting again sends nothing more.
    assert!(db.thread_start(&ravi, &w, &id).await.unwrap().1.is_none());

    // While HR Desk works, the assistant waits. Its answer is handed over once.
    assert_eq!(db.dispatch_threads().await.unwrap(), 0);
    let hr_job = super::super::super::handoff_tests::claim(&db, &hr_job).await;
    db.chat_finish(&hr_job, "Which manager approves your leave?", "complete")
        .await
        .unwrap();
    assert_eq!(db.dispatch_threads().await.unwrap(), 1);
    assert_eq!(db.dispatch_threads().await.unwrap(), 0, "one run at a time");
    let run = queued(&db, &own_room).await;
    assert_eq!(run.agent, mine);
    let notes = db.threads(&ravi, &w).await.unwrap()["threads"][0]["notes"].clone();
    assert_eq!(notes["chat"], own_room.as_str());
    let handed = one(
        &db,
        "SELECT string_agg(content,' ') FROM relay_chat_messages WHERE thread=$1 AND kind='user'",
        vec![notes["thread"].as_str().unwrap().into()],
    )
    .await;
    assert!(
        handed
            .as_str()
            .unwrap()
            .contains("Which manager approves your leave?")
    );

    // The assistant answers as Ravi; a retried send posts once and asks HR Desk again.
    let sent = reply(&db, &run, &id, "manager", "Priya Sharma approves it.").await;
    assert_eq!(sent["state"], "sent");
    let again = reply(&db, &run, &id, "manager", "Priya Sharma approves it.").await;
    assert_eq!(again["message"], sent["message"]);
    let posted = stored(&db, sent["message"].as_str().unwrap()).await;
    assert_eq!(posted["author"], ravi.id.as_str());
    assert_eq!(posted["kind"], "user");
    assert_eq!(posted["chat"], chat.as_str());
    assert_eq!(posted["target"], hr.as_str());
    assert_eq!(posted["assisted"]["by"], "assistant");
    assert_eq!(thread(&db, &id).await["sent"], 2);

    // Stopped: the next answer is not handed over, and nothing can be sent.
    db.thread_set(&ravi, &w, &id, "stopped").await.unwrap();
    let hr_again = queued(&db, &chat).await;
    db.chat_finish(&hr_again, "Approved. Enjoy your leave.", "complete")
        .await
        .unwrap();
    assert_eq!(db.dispatch_threads().await.unwrap(), 0);
    assert_eq!(
        reply(&db, &run, &id, "late", "Thanks").await["state"],
        "invalid"
    );
    db.chat_finish(&run, "I told HR Desk that Priya approves.", "complete")
        .await
        .unwrap();
    // Resumed: the answer that came meanwhile goes to the assistant.
    db.thread_set(&ravi, &w, &id, "running").await.unwrap();
    assert_eq!(db.dispatch_threads().await.unwrap(), 1);
    let last = queued(&db, &own_room).await;
    let done = db
        .finish_thread(
            &last,
            &serde_json::from_value(json!({"thread":id,"summary":"Leave approved."})).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(done["state"], "stopped");
    assert_eq!(
        thread(&db, &id).await["detail"],
        "Finished: Leave approved."
    );
    db.chat_finish(&last, "Your leave is approved.", "complete")
        .await
        .unwrap();

    // Deleted: the assistant's notes go; Ravi's messages to HR Desk stay.
    db.thread_set(&ravi, &w, &id, "deleted").await.unwrap();
    assert_eq!(db.threads(&ravi, &w).await.unwrap()["threads"], json!([]));
    let left = one(
        &db,
        "SELECT count(*) FROM relay_chat_messages WHERE (id=$1 OR thread=$1) AND state<>'deleted'",
        vec![notes["thread"].as_str().unwrap().into()],
    )
    .await;
    assert_eq!(left, 0);
    assert_eq!(
        stored(&db, &id).await["content"],
        "Hi, I'd like leave from 22 to 26 December."
    );
    let audited = one(
        &db,
        "SELECT count(*) FROM relay_audit WHERE target=$1 AND action LIKE 'assistant_thread_%'",
        vec![id.clone().into()],
    )
    .await;
    assert_eq!(audited, 4, "started, stopped, resumed, deleted");
}

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn only_the_member_sees_who_wrote_a_message_and_only_they_run_the_thread() {
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
    let hers = db
        .ensure_assistant(&priya, &w, "fixture", "openai")
        .await
        .unwrap();
    let own_room = room(&db, &mine).await;

    // Not with someone else's assistant, and not with yourself.
    assert!(
        db.thread_create(&ravi, &w, &new_thread("agent", &hers))
            .await
            .is_err()
    );
    assert!(
        db.thread_create(&ravi, &w, &new_thread("user", &ravi.id))
            .await
            .is_err()
    );

    let input = new_thread("user", &priya.id);
    let (chat, job) = db.thread_create(&ravi, &w, &input).await.unwrap();
    assert!(job.is_none(), "a person is not an agent to run");
    assert_eq!(stored(&db, &input.id).await["assisted"]["by"], "owner");
    // Ravi sees the record on his message; Priya sees an ordinary message from Ravi.
    let shown = |page: Value| {
        page["messages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["id"] == input.id.as_str())
            .unwrap()
            .clone()
    };
    let his = shown(db.chat_messages(&ravi, &w, &chat).await.unwrap());
    assert_eq!(his["assisted"]["title"], "Leave in December");
    let hers_view = shown(db.chat_messages(&priya, &w, &chat).await.unwrap());
    assert!(hers_view["assisted"].is_null());
    assert_eq!(hers_view["author"], ravi.id.as_str());

    // One running thread with the same person; Priya can't stop Ravi's.
    let second = db
        .thread_create(&ravi, &w, &new_thread("user", &priya.id))
        .await
        .unwrap_err();
    assert!(second.to_string().contains("already have a running thread"));
    assert!(
        db.thread_set(&priya, &w, &input.id, "stopped")
            .await
            .is_err()
    );
    assert_eq!(db.threads(&priya, &w).await.unwrap()["threads"], json!([]));

    // Priya answers: the assistant is handed it, and answers as Ravi.
    assert_eq!(db.dispatch_threads().await.unwrap(), 0);
    db.chat_send(
        &priya,
        &w,
        &chat,
        &message("Fine by me. Who covers for you?", None),
    )
    .await
    .unwrap();
    assert_eq!(db.dispatch_threads().await.unwrap(), 1);
    let run = queued(&db, &own_room).await;
    let sent = reply(&db, &run, &input.id, "cover", "Arjun covers for me.").await;
    assert_eq!(sent["state"], "sent");
    let posted = stored(&db, sent["message"].as_str().unwrap()).await;
    assert_eq!(posted["author"], ravi.id.as_str());
    assert!(posted["target"].is_null());

    // Priya's assistant can't write in Ravi's thread.
    db.chat_send(
        &priya,
        &w,
        &room(&db, &hers).await,
        &message("Hello", Some(&hers)),
    )
    .await
    .unwrap();
    let other = queued(&db, &room(&db, &hers).await).await;
    assert_eq!(
        reply(&db, &other, &input.id, "x", "Hi").await["state"],
        "invalid"
    );

    // At the limit the thread stops instead of sending.
    execute(
        &db.db,
        "UPDATE relay_work_threads SET sent=$2 WHERE id=$1",
        vec![input.id.clone().into(), LIMIT.into()],
    )
    .await
    .unwrap();
    assert_eq!(
        reply(&db, &run, &input.id, "more", "One more").await["state"],
        "invalid"
    );
    let stopped = thread(&db, &input.id).await;
    assert_eq!(stopped["state"], "stopped");
    assert!(stopped["detail"].as_str().unwrap().contains("30 messages"));
}

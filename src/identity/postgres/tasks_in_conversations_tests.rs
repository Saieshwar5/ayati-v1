//! The owner's one rule (schema 151): a task run happens in the task's own
//! conversation, as the person it belongs to, exactly like that person asking the
//! agent there. A channel task reads the channel; a trigger runs where it was set
//! up, or in its person's direct chat with the agent; nothing is copied anywhere.
use super::super::attention_tests::{ask, page};
use super::super::control_tests::setup;
use super::super::event_tests::{Setup, setup as trigger_setup};
use super::super::memory::{Shelf, access};
use super::super::tests::*;
use super::super::*;
use super::lifetime_tests::{claim, due_now, last_run, one, task, wakeups};
use crate::identity::chat::model::Job;
use serde_json::json;

/// The messages `person` sees in `chat`.
async fn shown(db: &PgStore, person: &User, w: &str, chat: &str) -> Vec<Value> {
    db.chat_messages(person, w, chat).await.unwrap()["messages"]
        .as_array()
        .unwrap()
        .clone()
}

/// A daily task the owner asks for in their group chat with the agent, with a
/// colleague in it who wrote there first. Returns the task and the colleague.
async fn channel_task(db: &mut PgStore, owner: &User, w: &str, job: &Job) -> (String, User) {
    db.pooled = true;
    let colleague = user(db, "channel-task-colleague").await;
    join(db, owner, w, &colleague, "Member").await;
    db.channel_members(
        owner,
        w,
        &job.chat,
        &[owner.id.clone(), colleague.id.clone()],
    )
    .await
    .unwrap();
    db.chat_send(
        &colleague,
        w,
        &job.chat,
        &message("Heads up: the Acme order ships on Friday", None),
    )
    .await
    .unwrap();
    let made = db
        .create_task(
            job,
            &task(
                json!({"title":"Morning stock check","repeat":{"unit":"day","at":"10:00"},
            "instructions":"List the items below their reorder level"}),
            ),
        )
        .await
        .unwrap();
    db.chat_finish(
        job,
        "Done. I'll check stock every day at 10:00.",
        "complete",
    )
    .await
    .unwrap();
    (made["id"].as_str().unwrap().to_owned(), colleague)
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_channel_task_runs_in_the_channel_as_its_asker_and_reads_the_channel() {
    let (mut db, owner, w, _, job) = setup("allow").await;
    let (id, colleague) = channel_task(&mut db, &owner, &w, &job).await;
    due_now(&db, &id).await;
    assert_eq!(db.dispatch_task_wakeups().await.unwrap(), 1);
    let (reply, brief) = last_run(&db, &id).await;
    let run = claim(&db, &reply).await;
    assert_eq!(
        (run.chat.as_str(), run.requester.as_str()),
        (job.chat.as_str(), owner.id.as_str()),
        "a job in the channel, as the person who asked"
    );
    assert!(
        brief.starts_with("Task run 1 · Morning stock check"),
        "{brief}"
    );
    // Its history is the channel's, other people's messages included.
    assert!(
        run.prompt.contains("the Acme order ships on Friday") && run.prompt.contains(&colleague.id),
        "{}",
        run.prompt
    );
    // It reads the channel's room shelf, and what it keeps wider waits for a person.
    let memory = access(&db.db, &run).await.unwrap();
    assert_eq!(memory.here, Some(Shelf::room(&run.agent, &job.chat)));
    assert!(memory.read.contains(&Shelf::room(&run.agent, &job.chat)));
    assert_eq!(memory.waits_because, Some("background"));
    let kept = crate::identity::control::broker::invoke(
        &db,
        &run,
        "remember",
        json!({"text":"Purchasing restocks rice on Mondays","kind":"practice","for":"workspace"}),
    )
    .await
    .unwrap();
    assert_eq!(kept["state"], "waiting", "{kept}");

    // A question goes to the person it runs for, in their Needs you.
    let question = db.ask_question(&run, &ask()).await.unwrap();
    let mine = db.attention(&owner, &w, &page("needs_you")).await.unwrap();
    assert!(
        mine["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["id"] == question["id"])
    );
    let theirs = db
        .attention(&colleague, &w, &page("needs_you"))
        .await
        .unwrap();
    assert!(
        !theirs["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["id"] == question["id"])
    );

    db.chat_finish(
        &run,
        "Rice is below its reorder level: 12 bags left.",
        "complete",
    )
    .await
    .unwrap();
    // The reply is in the channel, in its main timeline, and nothing else is posted.
    let seen = shown(&db, &colleague, &w, &job.chat).await;
    let answer = seen.iter().find(|m| m["id"] == reply.as_str()).unwrap();
    assert_eq!(
        (answer["content"].as_str(), answer["thread"].as_str()),
        (Some("Rice is below its reorder level: 12 bags left."), None)
    );
    let agent_messages = seen.iter().filter(|m| m["kind"] == "agent").count();
    assert_eq!(
        agent_messages, 2,
        "the first answer and the run's reply, nothing copied"
    );
    // Its input reads as one line about the task, for everyone there.
    let input = wakeups(&db, &id).await[0]["message"].clone();
    let line = seen.iter().find(|m| m["id"] == input).unwrap();
    assert_eq!(line["follow_up"]["title"], "Morning stock check");
    assert_eq!(line["follow_up"]["wakeup"], "repeat");
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_run_with_nothing_to_tell_shows_nothing_in_the_channel() {
    let (mut db, owner, w, _, job) = setup("allow").await;
    let (id, colleague) = channel_task(&mut db, &owner, &w, &job).await;
    let before = shown(&db, &colleague, &w, &job.chat).await.len();
    due_now(&db, &id).await;
    assert_eq!(db.dispatch_task_wakeups().await.unwrap(), 1);
    let (reply, _) = last_run(&db, &id).await;
    let run = claim(&db, &reply).await;
    db.chat_finish(&run, "NO_CHANGE", "complete").await.unwrap();
    let after = shown(&db, &colleague, &w, &job.chat).await;
    let visible: Vec<&Value> = after
        .iter()
        .filter(|m| m["state"] != "deleted" && m["quiet"] != true)
        .collect();
    assert_eq!(
        visible.len(),
        before,
        "neither its input nor its reply shows"
    );
    assert_eq!(wakeups(&db, &id).await[0]["quiet"], true);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_run_for_someone_who_left_the_channel_is_blocked() {
    let (mut db, owner, w, _, job) = setup("allow").await;
    let (id, _) = channel_task(&mut db, &owner, &w, &job).await;
    execute(
        &db.db,
        "DELETE FROM relay_chat_people WHERE workspace=$1 AND chat=$2 AND user_id=$3",
        vec![
            w.clone().into(),
            job.chat.clone().into(),
            owner.id.clone().into(),
        ],
    )
    .await
    .unwrap();
    due_now(&db, &id).await;
    assert_eq!(db.dispatch_task_wakeups().await.unwrap(), 0);
    let blocked = wakeups(&db, &id)
        .await
        .into_iter()
        .find(|k| k["state"] == "blocked")
        .expect("the run is blocked");
    assert!(
        blocked["detail"].as_str().unwrap().contains("blocked"),
        "{blocked}"
    );
    assert_eq!(
        one(
            &db,
            "SELECT count(*) AS n FROM relay_chat_jobs WHERE task=$1 AND reply<>$2",
            vec![id.into(), job.reply.clone().into()]
        )
        .await["n"],
        0,
        "no work started"
    );
}

/// The queued run of the setup's current trigger, claimed as a worker would.
async fn event_run(s: &Setup) -> Job {
    let reply: String = query(&s.db.db, "SELECT j.reply FROM relay_chat_jobs j JOIN relay_event_deliveries d ON d.workspace=j.workspace AND d.reply=j.reply WHERE j.state='queued' AND d.trigger_id=$1 ORDER BY j.sequence DESC LIMIT 1", vec![s.input.id.clone().into()])
        .await.unwrap().unwrap().try_get("", "reply").unwrap();
    claim(&s.db, &reply).await
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_trigger_runs_in_its_conversation_or_its_persons_direct_chat() {
    let mut s = trigger_setup().await;
    s.save().await;
    s.accept("first", "delayed").await;
    assert_eq!(s.db.dispatch_events().await.unwrap(), 1);
    let run = event_run(&s).await;
    assert_eq!(
        (run.chat.as_str(), run.requester.as_str()),
        (s.c.as_str(), s.owner.id.as_str()),
        "in the chosen conversation, as the person who set it up"
    );
    s.db.chat_finish(&run, "Order 42 is late.", "complete")
        .await
        .unwrap();
    let seen = shown(&s.db, &s.owner, &s.w, &s.c).await;
    let reply = seen.iter().find(|m| m["id"] == run.reply.as_str()).unwrap();
    assert_eq!(reply["content"], "Order 42 is late.");
    let input = one(
        &s.db,
        "SELECT message FROM relay_event_deliveries WHERE reply=$1",
        vec![run.reply.clone().into()],
    )
    .await["message"]
        .clone();
    let line = seen.iter().find(|m| m["id"] == input).unwrap();
    assert_eq!(line["follow_up"]["wakeup"], "event");
    assert_eq!(line["follow_up"]["title"], "Review order change");

    // Without a conversation it runs in the person's direct chat with the agent.
    s.input.id = uuid::Uuid::new_v4().to_string();
    s.input.name = "Private order review".into();
    s.input.chat = None;
    let saved = s.save().await;
    assert_eq!(saved["chat_direct"], true);
    s.accept("second", "delayed").await;
    // The event reaches both triggers; each runs in its own place.
    assert_eq!(s.db.dispatch_events().await.unwrap(), 2);
    let direct = event_run(&s).await;
    let place = one(&s.db, "SELECT c.kind,c.direct_key IS NOT NULL AS keyed,(SELECT jsonb_agg(user_id) FROM relay_chat_people p WHERE p.chat=c.id) AS people,(SELECT jsonb_agg(agent) FROM relay_chat_agents x WHERE x.chat=c.id) AS agents FROM relay_chats c WHERE c.id=$1", vec![direct.chat.clone().into()]).await;
    assert_eq!(
        place,
        json!({"kind":"direct","keyed":true,"people":[s.owner.id],"agents":[s.a]})
    );
    assert_eq!(direct.requester, s.owner.id);
    assert_eq!(saved["chat"], direct.chat.as_str());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_trigger_whose_person_left_its_conversation_runs_nothing() {
    let s = trigger_setup().await;
    s.save().await;
    execute(
        &s.db.db,
        "DELETE FROM relay_chat_people WHERE workspace=$1 AND chat=$2 AND user_id=$3",
        vec![
            s.w.clone().into(),
            s.c.clone().into(),
            s.owner.id.clone().into(),
        ],
    )
    .await
    .unwrap();
    // The intake checks the person may still ask the agent there: no run is admitted.
    s.accept("first", "delayed").await;
    assert_eq!(s.db.dispatch_events().await.unwrap(), 0);
    let admitted = one(
        &s.db,
        "SELECT count(*) AS n FROM relay_event_deliveries WHERE trigger_id=$1",
        vec![s.input.id.clone().into()],
    )
    .await;
    assert_eq!(admitted["n"], 0);
    assert!(
        s.db.chat_messages(&s.owner, &s.w, &s.c).await.is_err(),
        "they can't see the conversation either"
    );
}

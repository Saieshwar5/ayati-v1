//! Schema 148: every event trigger has a task. A delivered event starts a run of
//! it in the trigger's conversation (schema 151), counted and shown with the task's
//! runs; NO_CHANGE shows nothing. Pausing, resuming and ending the task and the
//! trigger go together.
use super::super::event_tests::{Setup, setup, setup_on};
use super::super::*;
use crate::identity::chat::model::Job;
use sea_orm_migration::MigratorTrait;
use serde_json::json;

/// The newest queued event job, claimed as a worker would.
async fn claim_latest(db: &PgStore) -> Job {
    let claim = uuid::Uuid::new_v4().to_string();
    let row = query(&db.db, "UPDATE relay_chat_jobs SET state='running',claim=$1,attempts=1,lease_until=floor(extract(epoch FROM clock_timestamp()))::bigint+300
        WHERE reply=(SELECT j.reply FROM relay_chat_jobs j JOIN relay_event_deliveries d ON d.workspace=j.workspace AND d.reply=j.reply WHERE j.state='queued' ORDER BY j.sequence DESC LIMIT 1) RETURNING payload",
        vec![claim.clone().into()]).await.unwrap().unwrap();
    let mut job: Job = serde_json::from_value(row.try_get("", "payload").unwrap()).unwrap();
    job.claim = Some(claim);
    job
}

async fn value(db: &PgStore, sql: &str, args: Vec<sea_orm::Value>) -> Value {
    query(&db.db, &format!("SELECT ({sql}) AS v"), args)
        .await
        .unwrap()
        .unwrap()
        .try_get("", "v")
        .unwrap()
}

/// Trigger `id`'s task with its wake-ups, and the trigger's conversation.
async fn task_of(db: &PgStore, id: &str) -> Value {
    value(db, "SELECT to_jsonb(t)||jsonb_build_object('trigger_chat',x.chat,
          'wakeups',(SELECT COALESCE(jsonb_agg(jsonb_build_object('kind',k.kind,'state',k.state,'run',k.run,'reply',k.reply,'quiet',k.quiet) ORDER BY k.run),'[]') FROM relay_task_wakeups k WHERE k.workspace=t.workspace AND k.task=t.id))
        FROM relay_event_triggers x JOIN relay_tasks t ON t.workspace=x.workspace AND t.id=x.task WHERE x.id=$1", vec![id.into()]).await
}

async fn trigger_state(db: &PgStore, id: &str) -> String {
    value(
        db,
        "SELECT to_jsonb(state) FROM relay_event_triggers WHERE id=$1",
        vec![id.into()],
    )
    .await
    .as_str()
    .unwrap()
    .to_owned()
}

/// Save a copy of the setup's trigger under a new id running in `chat`.
async fn another(s: &mut Setup, name: &str, chat: Option<String>) -> String {
    s.input.id = uuid::Uuid::new_v4().to_string();
    s.input.name = name.into();
    s.input.chat = chat;
    s.save().await;
    s.input.id.clone()
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn schema_148_gives_every_trigger_a_task_whose_runs_happen_in_its_conversation() {
    let url = std::env::var("RELAY_TEST_DATABASE_URL").unwrap();
    let admin = PgStore::connect(&url).await.unwrap();
    let schema = format!("relay_trigger_tasks_{}", uuid::Uuid::new_v4().simple());
    admin
        .db
        .execute_unprepared(&format!("CREATE SCHEMA {schema}"))
        .await
        .unwrap();
    let mut scoped = reqwest::Url::parse(&url).unwrap();
    scoped
        .query_pairs_mut()
        .append_pair("options", &format!("-c search_path={schema}"));
    let db = PgStore::connect(scoped.as_str()).await.unwrap();
    super::super::migration::Migrator::up(&db.db, Some(147))
        .await
        .unwrap();
    let mut s = setup_on(db).await;
    let c = s.c.clone();

    // A trigger in the conversation ran once before the upgrade, with an
    // automatic task as event runs had.
    let remember = another(&mut s, "Review order change", Some(c.clone())).await;
    s.accept("first", "delayed").await;
    assert_eq!(s.db.dispatch_events().await.unwrap(), 1);
    let old = claim_latest(&s.db).await;
    let automatic = value(
        &s.db,
        "SELECT to_jsonb(task) FROM relay_chat_jobs WHERE reply=$1",
        vec![old.reply.clone().into()],
    )
    .await;
    assert!(automatic.is_string(), "event runs had automatic tasks");
    s.db.chat_finish(&old, "One order is late.", "complete")
        .await
        .unwrap();
    // One in the person's direct chat that is paused, a blocked one, a removed one.
    let paused = another(&mut s, "Quiet check", None).await;
    s.state(&s.change(1, false)).await;
    let blocked = another(&mut s, "Blocked check", Some(c.clone())).await;
    execute(&s.db.db, "UPDATE relay_event_triggers SET state='blocked',detail='Source access changed.' WHERE id=$1", vec![blocked.clone().into()]).await.unwrap();
    let removed = another(&mut s, "Old check", None).await;
    let mut end = s.change(1, false);
    end.remove = true;
    s.state(&end).await;

    assert!(s.db.verify_schema().await.is_err());
    s.db.migrate().await.unwrap();
    s.db.migrate().await.unwrap();
    s.db.verify_schema().await.unwrap();

    let r = task_of(&s.db, &remember).await;
    assert_eq!(
        (
            r["kind"].as_str(),
            r["origin"].as_str(),
            r["title"].as_str()
        ),
        (
            Some("commitment"),
            Some("event"),
            Some("Review order change")
        )
    );
    assert_eq!(r["instructions"], "Review this change and report blockers.");
    assert_eq!(r["requester"], s.owner.id.as_str());
    assert_eq!(r["chat"], c.as_str(), "it belongs where the trigger runs");
    assert!(r.get("room").is_none(), "no work room (schema 151)");
    assert_eq!(r["runs"], 1);
    assert_eq!(
        r["wakeups"],
        json!([{"kind":"event","state":"started","run":1,"reply":old.reply,"quiet":false}])
    );
    assert_eq!(
        value(
            &s.db,
            "SELECT to_jsonb(task) FROM relay_chat_jobs WHERE reply=$1",
            vec![old.reply.clone().into()]
        )
        .await,
        r["id"],
        "its earlier run belongs to the task"
    );
    assert_eq!(
        value(
            &s.db,
            "SELECT to_jsonb(count(*)) FROM relay_tasks WHERE id=$1",
            vec![automatic.as_str().unwrap().into()]
        )
        .await,
        0,
        "the emptied automatic task is gone"
    );
    let p = task_of(&s.db, &paused).await;
    assert_eq!(p["chat"], p["trigger_chat"]);
    assert_ne!(
        p["chat"],
        c.as_str(),
        "the person's direct chat with the agent"
    );
    assert_eq!(p["paused_reason"], "Its event trigger is paused.");
    let b = task_of(&s.db, &blocked).await;
    assert_eq!(
        (b["paused_reason"].as_str(), b["chat"].as_str()),
        (Some("Source access changed."), Some(c.as_str()))
    );
    let d = task_of(&s.db, &removed).await;
    assert_eq!(d["outcome"], "done");

    // The next delivery is the task's next run, in its conversation, as its person;
    // the reply stays there and nothing is copied.
    s.accept("second", "delayed").await;
    assert_eq!(s.db.dispatch_events().await.unwrap(), 1);
    let run = claim_latest(&s.db).await;
    assert_eq!(
        (run.chat.as_str(), run.requester.as_str()),
        (c.as_str(), s.owner.id.as_str())
    );
    assert!(
        run.prompt.contains("This run belongs to your task") && run.prompt.contains("NO_CHANGE"),
        "{}",
        run.prompt
    );
    s.db.chat_finish(&run, "Two orders are late.", "complete")
        .await
        .unwrap();
    let reply = value(
        &s.db,
        "SELECT to_jsonb(m) FROM relay_chat_messages m WHERE id=$1",
        vec![run.reply.clone().into()],
    )
    .await;
    assert_eq!(
        (reply["chat"].as_str(), reply["content"].as_str()),
        (Some(c.as_str()), Some("Two orders are late."))
    );
    // A run with nothing to report shows nothing and counts as no change.
    s.accept("third", "delayed").await;
    assert_eq!(s.db.dispatch_events().await.unwrap(), 1);
    let quiet = claim_latest(&s.db).await;
    s.db.chat_finish(&quiet, "NO_CHANGE", "complete")
        .await
        .unwrap();
    assert_eq!(
        value(
            &s.db,
            "SELECT to_jsonb(state) FROM relay_chat_messages WHERE id=$1",
            vec![quiet.reply.clone().into()]
        )
        .await,
        "deleted"
    );
    let after = task_of(&s.db, &remember).await;
    assert_eq!(after["runs"], 3);
    assert_eq!(after["wakeups"][2]["quiet"], true);
    let details =
        s.db.task_details(&s.owner, &s.w, after["id"].as_str().unwrap())
            .await
            .unwrap();
    assert_eq!(details["runs"][0]["state"], "no_change", "{details}");
    assert_eq!(details["runs"][1]["state"], "done");
    assert_eq!(details["trigger"]["state"], "enabled");
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_trigger_and_its_task_pause_resume_and_end_together() {
    let s = setup().await;
    s.save().await;
    let t = task_of(&s.db, &s.input.id).await;
    let task = t["id"].as_str().unwrap().to_owned();
    assert_eq!(t["origin"], "event", "a new trigger has its task at once");

    // Deliveries wait while the last run is still going, as before.
    s.accept("one", "delayed").await;
    assert_eq!(s.db.dispatch_events().await.unwrap(), 1);
    s.accept("two", "delayed").await;
    assert_eq!(
        s.db.dispatch_events().await.unwrap(),
        0,
        "one unfinished run per trigger"
    );
    let first = claim_latest(&s.db).await;
    s.db.chat_finish(&first, "Order one is late.", "complete")
        .await
        .unwrap();
    assert_eq!(
        s.db.dispatch_events().await.unwrap(),
        1,
        "the waiting delivery runs next"
    );
    let second = claim_latest(&s.db).await;
    assert_eq!(task_of(&s.db, &s.input.id).await["runs"], 2);

    // The agent can't change a trigger's reviewed instructions or give it times.
    let change = |instructions: Option<&str>, paused: Option<bool>| super::changes::ChangeTask {
        task: task.clone(),
        title: None,
        done_when: None,
        repeat: None,
        first_at: None,
        until: None,
        max_runs: None,
        paused,
        instructions: instructions.map(str::to_string),
    };
    assert!(
        s.db.change_task(&second, &change(Some("Do something else."), None))
            .await
            .is_err()
    );

    // The calendar lists the trigger once, linking to its task, and its runs as the task's.
    let now = super::runs::now_ms(&s.db.db).await.unwrap();
    let calendar =
        s.db.activity_calendar(&s.owner, &s.w, now - 86_400_000, now + 86_400_000, None)
            .await
            .unwrap();
    let triggers: Vec<&Value> = calendar["untimed"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["kind"] == "trigger")
        .collect();
    assert_eq!(triggers.len(), 1);
    assert_eq!(triggers[0]["task"], task.as_str());
    assert!(
        calendar["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["kind"] == "run"
                && i["task"] == task.as_str()
                && i["reply"] == first.reply.as_str()),
        "{calendar}"
    );
    s.db.chat_finish(&second, "Order two is late.", "complete")
        .await
        .unwrap();

    // Pausing the task pauses the trigger; it resumes only on the agent page.
    s.db.task_set_paused(&s.owner, &s.w, &task, true)
        .await
        .unwrap();
    assert_eq!(trigger_state(&s.db, &s.input.id).await, "paused");
    let refused =
        s.db.task_set_paused(&s.owner, &s.w, &task, false)
            .await
            .unwrap_err();
    assert!(refused.to_string().contains("agent's page"), "{refused}");
    s.accept("three", "delayed").await;
    assert_eq!(
        s.db.dispatch_events().await.unwrap(),
        0,
        "a paused trigger runs nothing"
    );
    // Resuming the trigger from its own controls resumes the task.
    let revision = value(
        &s.db,
        "SELECT to_jsonb(revision) FROM relay_event_triggers WHERE id=$1",
        vec![s.input.id.clone().into()],
    )
    .await
    .as_i64()
    .unwrap();
    s.state(&s.change(revision, true)).await;
    assert!(task_of(&s.db, &s.input.id).await["paused_ms"].is_null());
    // Pausing the trigger pauses the task.
    s.state(&s.change(revision + 1, false)).await;
    let paused = task_of(&s.db, &s.input.id).await;
    assert_eq!(paused["paused_reason"], "Its event trigger was paused.");
    s.state(&s.change(revision + 2, true)).await;

    // The agent pausing it in chat pauses the trigger too.
    s.accept("four", "delayed").await;
    assert_eq!(s.db.dispatch_events().await.unwrap(), 1);
    let fourth = claim_latest(&s.db).await;
    s.db.change_task(&fourth, &change(None, Some(true)))
        .await
        .unwrap();
    assert_eq!(trigger_state(&s.db, &s.input.id).await, "paused");
    assert!(
        s.db.change_task(&fourth, &change(None, Some(false)))
            .await
            .is_err()
    );
    s.db.chat_finish(&fourth, "Done.", "complete")
        .await
        .unwrap();

    // "Not a task" can't stop a trigger; ending the task removes it for good.
    assert!(
        s.db.task_close(&s.owner, &s.w, &task, "not_task", None)
            .await
            .is_err()
    );
    s.db.task_close(&s.owner, &s.w, &task, "cancelled", None)
        .await
        .unwrap();
    assert_eq!(trigger_state(&s.db, &s.input.id).await, "removed");
    assert_eq!(task_of(&s.db, &s.input.id).await["outcome"], "cancelled");
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn removing_a_trigger_on_the_agent_page_ends_its_task() {
    let s = setup().await;
    s.save().await;
    let mut end = s.change(1, false);
    end.remove = true;
    s.state(&end).await;
    let t = task_of(&s.db, &s.input.id).await;
    assert_eq!(
        (t["outcome"].as_str(), t["result"].as_str()),
        (Some("done"), Some("Its event trigger was removed."))
    );
}

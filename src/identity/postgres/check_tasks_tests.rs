//! Schema 149: an assistant's regular check is the task "Regular check" for its
//! person. A run first looks in code; with nothing new it records no change and
//! calls no model; with something new the assistant runs in the "Regular check"
//! thread of its chat, as checks did.
use super::super::assistants::settings::SaveSettings;
use super::super::tests::*;
use super::super::*;
use crate::identity::chat::model::NewChat;
use sea_orm_migration::MigratorTrait;
use serde_json::json;

async fn value(db: &PgStore, sql: &str, args: Vec<sea_orm::Value>) -> Value {
    query(&db.db, &format!("SELECT to_jsonb(({sql})) AS v"), args)
        .await
        .unwrap()
        .unwrap()
        .try_get("", "v")
        .unwrap()
}

/// Make the check task's next run due now.
async fn due(db: &PgStore, task: &str) {
    execute(&db.db, "UPDATE relay_task_wakeups SET due_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint-1000 WHERE task=$1 AND state='pending'", vec![task.into()]).await.unwrap();
}

fn checks(on: bool, checklist: &str) -> SaveSettings {
    SaveSettings {
        checks: Some(on),
        every_minutes: Some(120),
        checklist: Some(checklist.into()),
        learn: None,
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn schema_149_turns_each_regular_check_into_a_task_that_looks_first_in_code() {
    let url = std::env::var("RELAY_TEST_DATABASE_URL").unwrap();
    let admin = PgStore::connect(&url).await.unwrap();
    let schema = format!("relay_check_tasks_{}", uuid::Uuid::new_v4().simple());
    admin
        .db
        .execute_unprepared(&format!("CREATE SCHEMA {schema}"))
        .await
        .unwrap();
    let mut scoped = reqwest::Url::parse(&url).unwrap();
    scoped
        .query_pairs_mut()
        .append_pair("options", &format!("-c search_path={schema}"));
    let mut db = PgStore::connect(scoped.as_str()).await.unwrap();
    super::super::migration::Migrator::up(&db.db, Some(148))
        .await
        .unwrap();
    db.pooled = true;
    let ravi = user(&db, "ravi").await;
    let priya = user(&db, "priya").await;
    let w = workspace(&db, &ravi).await;
    join(&db, &ravi, &w, &priya, "Member").await;
    let his = db
        .ensure_assistant(&ravi, &w, "fixture", "openai")
        .await
        .unwrap();
    db.ensure_assistant(&priya, &w, "fixture", "openai")
        .await
        .unwrap();
    let room = value(
        &db,
        "SELECT chat FROM relay_chat_agents WHERE agent=$1",
        vec![his.clone().into()],
    )
    .await
    .as_str()
    .unwrap()
    .to_owned();
    // Ravi's checks are on with a checklist; Priya's were turned on and off again.
    db.save_assistant_settings(&ravi, &w, &checks(true, ""))
        .await
        .unwrap();
    db.save_assistant_settings(&priya, &w, &checks(true, "Late orders?"))
        .await
        .unwrap();
    db.save_assistant_settings(&priya, &w, &checks(false, "Late orders?"))
        .await
        .unwrap();
    let next = value(
        &db,
        "SELECT next_ms FROM relay_assistant_settings WHERE user_id=$1",
        vec![ravi.id.clone().into()],
    )
    .await
    .as_i64()
    .unwrap();

    assert!(db.verify_schema().await.is_err());
    db.migrate().await.unwrap();
    db.migrate().await.unwrap();
    db.verify_schema().await.unwrap();

    let task = value(&db, "SELECT to_jsonb(t)||jsonb_build_object('next',(SELECT jsonb_build_object('kind',k.kind,'due_ms',k.due_ms) FROM relay_task_wakeups k WHERE k.workspace=t.workspace AND k.task=t.id AND k.state='pending'))
        FROM relay_assistant_settings c JOIN relay_tasks t ON t.workspace=c.workspace AND t.id=c.task WHERE c.user_id=$1", vec![ravi.id.clone().into()]).await;
    assert_eq!(
        (
            task["title"].as_str(),
            task["origin"].as_str(),
            task["kind"].as_str()
        ),
        (Some("Regular check"), Some("check"), Some("commitment"))
    );
    assert_eq!(
        (task["precheck"].as_str(), task["repeat"].clone()),
        (Some("messages"), json!({"unit":"minute","every":120}))
    );
    assert_eq!(
        (
            task["requester"].as_str(),
            task["agent"].as_str(),
            task["chat"].as_str()
        ),
        (
            Some(ravi.id.as_str()),
            Some(his.as_str()),
            Some(room.as_str())
        )
    );
    assert!(task["instructions"].is_null() && task["room"].is_null());
    assert_eq!(
        task["next"],
        json!({"kind":"repeat","due_ms":next}),
        "its next check stays"
    );
    assert_eq!(
        value(
            &db,
            "SELECT COALESCE(task,'none') FROM relay_assistant_settings WHERE user_id=$1",
            vec![priya.id.clone().into()]
        )
        .await,
        "none",
        "checks that are off have no task"
    );
    let id = task["id"].as_str().unwrap().to_owned();
    let jobs = "SELECT count(*) FROM relay_chat_jobs WHERE payload->>'chat'=$1";

    // Nothing new and no checklist: no model call, recorded as no change.
    due(&db, &id).await;
    assert_eq!(db.dispatch_task_wakeups().await.unwrap(), 0);
    assert_eq!(value(&db, jobs, vec![room.clone().into()]).await, 0);
    let quiet = value(&db, "SELECT jsonb_build_object('state',state,'quiet',quiet) FROM relay_task_wakeups WHERE task=$1 AND state<>'pending'", vec![id.clone().into()]).await;
    assert_eq!(quiet, json!({"state":"started","quiet":true}));

    // Priya writes to Ravi: the next check runs, in the Regular check thread.
    let chat = db
        .chat_create(
            &priya,
            &w,
            &NewChat {
                id: uuid::Uuid::new_v4().to_string(),
                name: "Direct".into(),
                kind: "direct".into(),
                description: String::new(),
                users: vec![ravi.id.clone()],
                agents: vec![],
            },
        )
        .await
        .unwrap();
    db.chat_send(
        &priya,
        &w,
        &chat,
        &message("Please sign the Acme order.", None),
    )
    .await
    .unwrap();
    due(&db, &id).await;
    assert_eq!(db.dispatch_task_wakeups().await.unwrap(), 1);
    let run = value(&db, "SELECT jsonb_build_object('task',j.task,'thread',m.thread,'content',m.content) FROM relay_chat_jobs j JOIN relay_chat_messages m ON m.workspace=j.workspace AND m.id=j.request_message WHERE j.payload->>'chat'=$1", vec![room.clone().into()]).await;
    assert_eq!(run["task"], id.as_str());
    assert!(
        run["content"]
            .as_str()
            .unwrap()
            .contains("Please sign the Acme order.")
    );
    let thread = value(
        &db,
        "SELECT thread FROM relay_assistant_threads WHERE agent=$1 AND automation='checks'",
        vec![his.clone().into()],
    )
    .await;
    assert_eq!(run["thread"], thread, "it runs in the Regular check thread");
}

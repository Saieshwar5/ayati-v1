//! Schema 150 drops the old schedule tables. A workspace with schedules from before
//! schema 147 upgrades through 147 (each becomes a task with its earlier runs) to
//! 150: no schedule table, column, function or trigger is left, and the converted
//! tasks keep their rules and runs and run through the task engine, since schema
//! 151 in their conversation.
use super::super::home_maintenance_tests::{claim, setup_on};
use super::super::*;
use crate::identity::chat::model::NewMessage;
use sea_orm_migration::MigratorTrait;
use serde_json::json;

/// Monday 23 November 2026, 09:30 India time.
const FIRST: i64 = 1_795_406_400_000;

struct Old<'a> {
    name: &'a str,
    chat: Option<&'a str>,
    interval: Option<i64>,
    state: &'a str,
    next: Option<i64>,
    detail: &'a str,
}

async fn schedule(db: &PgStore, w: &str, agent: &str, person: &str, old: Old<'_>) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    execute(&db.db, "INSERT INTO relay_agent_schedules(workspace,id,agent,principal,chat,name,content,first_ms,interval_seconds,timezone,request_hash,next_ms,state,detail)
        VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,'Asia/Kolkata',$2,$10,$11,$12)",
        vec![w.into(), id.clone().into(), agent.into(), person.into(), old.chat.map(str::to_string).into(), old.name.into(),
            format!("Do the {}.", old.name.to_lowercase()).into(), FIRST.into(), old.interval.into(), old.next.into(), old.state.into(), old.detail.into()]).await.unwrap();
    id
}

/// Schedule tables, columns, functions, triggers and constraints left in this schema.
async fn schedule_objects(db: &PgStore) -> Vec<String> {
    rows(&db.db, "SELECT 'table '||table_name AS n FROM information_schema.tables WHERE table_schema=current_schema() AND table_name ILIKE '%schedule%'
        UNION ALL SELECT 'column '||table_name||'.'||column_name FROM information_schema.columns WHERE table_schema=current_schema() AND column_name ILIKE '%schedule%'
        UNION ALL SELECT 'function '||proname FROM pg_proc WHERE pronamespace=current_schema()::regnamespace AND (proname ILIKE '%schedule%' OR prosrc ILIKE '%relay_agent_schedules%' OR prosrc ILIKE '%relay_schedule_%')
        UNION ALL SELECT 'trigger '||tgname FROM pg_trigger t JOIN pg_class c ON c.oid=t.tgrelid WHERE c.relnamespace=current_schema()::regnamespace AND NOT t.tgisinternal AND tgname ILIKE '%schedule%'
        UNION ALL SELECT 'constraint '||conname FROM pg_constraint WHERE connamespace=current_schema()::regnamespace AND (conname ILIKE '%schedule%' OR pg_get_constraintdef(oid) ILIKE '%schedule%')", vec![])
        .await.unwrap().into_iter().map(|r| r.try_get("", "n").unwrap()).collect()
}

/// The task a schedule named `title` became.
async fn task(db: &PgStore, title: &str) -> Value {
    query(&db.db, "SELECT to_jsonb(t)||jsonb_build_object('next',(SELECT jsonb_build_object('kind',k.kind,'due_ms',k.due_ms) FROM relay_task_wakeups k WHERE k.workspace=t.workspace AND k.task=t.id AND k.state='pending'),
          'chat_direct',(SELECT c.kind='direct' AND c.direct_key IS NOT NULL FROM relay_chats c WHERE c.workspace=t.workspace AND c.id=t.chat),
          'chat_people',(SELECT jsonb_agg(p.user_id) FROM relay_chat_people p WHERE p.workspace=t.workspace AND p.chat=t.chat),
          'chat_agents',(SELECT jsonb_agg(x.agent) FROM relay_chat_agents x WHERE x.workspace=t.workspace AND x.chat=t.chat)) AS v
        FROM relay_tasks t WHERE t.title=$1", vec![title.into()])
        .await.unwrap().map(|r| r.try_get("", "v").unwrap()).unwrap_or(Value::Null)
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn schema_150_drops_schedules_and_converted_tasks_keep_running_in_their_conversation() {
    let url = std::env::var("RELAY_TEST_DATABASE_URL").unwrap();
    let admin = PgStore::connect(&url).await.unwrap();
    let schema = format!("relay_drop_schedules_{}", uuid::Uuid::new_v4().simple());
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
    super::super::migration::Migrator::up(&db.db, Some(146))
        .await
        .unwrap();
    let (db, owner, w, first, _, _, panel) = setup_on(db).await;
    let (a, c) = (first.agent.clone(), first.chat.clone());
    let soon = super::runs::now_ms(&db.db).await.unwrap() + 3_600_000;
    let old = |name, chat, interval, state, next, detail| Old {
        name,
        chat,
        interval,
        state,
        next,
        detail,
    };
    let daily = schedule(
        &db,
        &w,
        &a,
        &owner.id,
        old(
            "Daily delivery review",
            Some(&c),
            Some(86_400),
            "enabled",
            Some(soon),
            "",
        ),
    )
    .await;
    schedule(
        &db,
        &w,
        &a,
        &owner.id,
        old(
            "Weekly sales summary",
            None,
            Some(604_800),
            "enabled",
            Some(soon),
            "",
        ),
    )
    .await;
    schedule(
        &db,
        &w,
        &a,
        &owner.id,
        old("Stock check", None, Some(7_200), "enabled", Some(soon), ""),
    )
    .await;
    schedule(
        &db,
        &w,
        &a,
        &owner.id,
        old("Queue check", None, Some(5_400), "enabled", Some(soon), ""),
    )
    .await;
    schedule(
        &db,
        &w,
        &a,
        &owner.id,
        old(
            "Supplier check",
            None,
            Some(129_600),
            "enabled",
            Some(soon),
            "",
        ),
    )
    .await;
    schedule(
        &db,
        &w,
        &a,
        &owner.id,
        old("Year end note", None, None, "enabled", Some(soon), ""),
    )
    .await;
    schedule(
        &db,
        &w,
        &a,
        &owner.id,
        old(
            "Paused report",
            None,
            Some(86_400),
            "paused",
            Some(soon),
            "",
        ),
    )
    .await;
    schedule(
        &db,
        &w,
        &a,
        &owner.id,
        old(
            "Blocked report",
            Some(&c),
            Some(86_400),
            "blocked",
            Some(soon),
            "Routine access changed.",
        ),
    )
    .await;
    schedule(
        &db,
        &w,
        &a,
        &owner.id,
        old("Launch note", None, None, "completed", None, ""),
    )
    .await;
    let home = schedule(
        &db,
        &w,
        &a,
        &owner.id,
        old(
            "Personal Home panel update",
            Some(&c),
            Some(60),
            "paused",
            Some(soon),
            "",
        ),
    )
    .await;
    execute(
        &db.db,
        "INSERT INTO relay_home_schedule_bindings VALUES($1,$2,$3)",
        vec![
            w.clone().into(),
            home.clone().into(),
            panel.id.clone().into(),
        ],
    )
    .await
    .unwrap();

    // The daily one ran once before the upgrade, in its work room, and the run had
    // an automatic task as schedule runs did.
    let tx = db.workspace_tx(&w).await.unwrap();
    let room = super::conversation_migration_tests::work_room(
        &tx,
        &w,
        "Daily delivery review",
        &owner.id,
        &a,
    )
    .await;
    execute(
        &tx,
        "UPDATE relay_agent_schedules SET room=$2 WHERE id=$1",
        vec![daily.clone().into(), room.clone().into()],
    )
    .await
    .unwrap();
    let input = NewMessage {
        reply_to: None,
        id: uuid::Uuid::new_v4().to_string(),
        content: "Daily delivery review\nDo the daily delivery review.".into(),
        agent: Some(a.clone()),
        agents: vec![],
        thread: None,
        attachments: vec![],
    };
    let ran = db
        .background_in(&tx, &owner, &w, &room, &input)
        .await
        .unwrap()
        .unwrap();
    execute(&tx, "INSERT INTO relay_schedule_occurrences(workspace,schedule,due_ms,message,reply) VALUES($1,$2,$3,$4,$5)",
        vec![w.clone().into(), daily.clone().into(), FIRST.into(), input.id.clone().into(), ran.reply.clone().into()]).await.unwrap();
    super::ensure_automatic(&tx, &ran, "schedule")
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let automatic: String = query(
        &db.db,
        "SELECT task FROM relay_chat_jobs WHERE reply=$1",
        vec![ran.reply.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "task")
    .unwrap();
    let job = claim(&db, &w, &ran.reply).await;
    db.chat_finish(&job, "One delivery was late.", "complete")
        .await
        .unwrap();

    // Through schema 149 the schedules become tasks; 150 drops what is left of them;
    // 151 runs each in its conversation.
    super::super::migration::Migrator::up(&db.db, Some(3))
        .await
        .unwrap();
    assert!(db.verify_schema().await.is_err());
    assert!(!schedule_objects(&db).await.is_empty());
    db.migrate().await.unwrap();
    db.migrate().await.unwrap();
    db.verify_schema().await.unwrap();
    assert_eq!(schedule_objects(&db).await, Vec::<String>::new());

    // Rules: whole days daily, whole weeks on the first run's weekday and time,
    // whole hours that divide a day at the same minute, anything else in minutes.
    // A converted schedule was set up by a person (no schedule origin is left).
    let rules = [
        (
            "Daily delivery review",
            json!({"unit":"day","every":1,"at":"09:30"}),
        ),
        (
            "Weekly sales summary",
            json!({"unit":"week","every":1,"days":[1],"at":"09:30"}),
        ),
        (
            "Stock check",
            json!({"unit":"hour","every":2,"from":"01:30"}),
        ),
        ("Queue check", json!({"unit":"minute","every":90})),
        ("Supplier check", json!({"unit":"minute","every":2160})),
        ("Year end note", Value::Null),
    ];
    for (title, rule) in rules {
        let t = task(&db, title).await;
        assert_eq!(t["repeat"], rule, "{t}");
        assert_eq!(
            (t["kind"].as_str(), t["origin"].as_str()),
            (Some("commitment"), Some("person"))
        );
        assert_eq!(
            (t["timezone"].as_str(), t["anchor_ms"].as_i64()),
            (Some("Asia/Kolkata"), Some(FIRST))
        );
        assert_eq!(t["requester"], owner.id.as_str());
        assert!(t["outcome"].is_null() && t["paused_ms"].is_null());
        let kind = if rule.is_null() {
            "follow_up"
        } else {
            "repeat"
        };
        assert_eq!(
            t["next"],
            json!({"kind": kind, "due_ms": soon}),
            "its next run is booked"
        );
        assert!(t["instructions"].as_str().unwrap().starts_with("Do the "));
    }
    // Where it runs: with a post target, the conversation it posted to; without
    // one, the person's direct chat with the agent (schema 151).
    let d = task(&db, "Daily delivery review").await;
    assert_eq!(d["chat"].as_str(), Some(c.as_str()));
    assert!(d.get("room").is_none());
    assert_eq!(
        (d["title"].as_str(), d["runs"].as_i64()),
        (Some("Daily delivery review"), Some(1))
    );
    let wk = task(&db, "Weekly sales summary").await;
    assert_eq!(wk["chat_direct"], true);
    assert_eq!(
        (wk["chat_people"].clone(), wk["chat_agents"].clone()),
        (json!([owner.id]), json!([a]))
    );
    assert_eq!(wk["runs"], 0);
    // Paused and blocked become paused tasks with their reason; completed, done.
    let p = task(&db, "Paused report").await;
    assert_eq!(
        (p["paused_reason"].as_str(), p["next"].is_null()),
        (Some("Paused before it became a task."), true)
    );
    let b = task(&db, "Blocked report").await;
    assert_eq!(
        (b["paused_reason"].as_str(), b["next"].is_null()),
        (Some("Routine access changed."), true)
    );
    assert_eq!(b["chat"], c.as_str());
    let done = task(&db, "Launch note").await;
    assert_eq!(done["outcome"], "done");
    assert!(done["closed_ms"].as_i64().is_some() && done["next"].is_null());
    // A Home panel's schedule never became a task.
    assert!(task(&db, "Personal Home panel update").await.is_null());

    // The old run belongs to the task now; its automatic task is gone.
    let linked: String = query(
        &db.db,
        "SELECT task FROM relay_chat_jobs WHERE reply=$1",
        vec![ran.reply.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "task")
    .unwrap();
    assert_eq!(linked, d["id"].as_str().unwrap());
    assert!(
        query(
            &db.db,
            "SELECT 1 AS x FROM relay_tasks WHERE id=$1",
            vec![automatic.into()]
        )
        .await
        .unwrap()
        .is_none()
    );
    // The old run reads as its task's run.
    let details = db.activity_request(&owner, &w, &ran.reply).await.unwrap();
    assert_eq!(details["task"]["id"], d["id"]);
    assert!(details.get("schedule").is_none());

    // Its next run starts through the task engine in its conversation; the reply
    // stays there and nothing is copied.
    execute(
        &db.db,
        "UPDATE relay_task_wakeups SET state='cancelled' WHERE state='pending' AND task<>$1",
        vec![d["id"].as_str().unwrap().into()],
    )
    .await
    .unwrap();
    execute(&db.db, "UPDATE relay_task_wakeups SET due_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint-1000 WHERE state='pending' AND task=$1", vec![d["id"].as_str().unwrap().into()]).await.unwrap();
    assert_eq!(db.dispatch_task_wakeups().await.unwrap(), 1);
    let reply: String = query(
        &db.db,
        "SELECT reply FROM relay_task_wakeups WHERE task=$1 AND state='started'",
        vec![d["id"].as_str().unwrap().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "reply")
    .unwrap();
    let run = claim(&db, &w, &reply).await;
    assert_eq!(
        db.activity_request(&owner, &w, &reply).await.unwrap()["kind"],
        "task",
        "Activity shows it as a task's run"
    );
    assert_eq!(run.chat, c);
    assert_ne!(run.chat, room);
    assert!(
        run.prompt
            .contains("What to do each run: Do the daily delivery review."),
        "{}",
        run.prompt
    );
    db.chat_finish(&run, "Two deliveries are late.", "complete")
        .await
        .unwrap();
    let posted = query(
        &db.db,
        "SELECT content FROM relay_chat_messages WHERE id=$1 AND chat=$2",
        vec![reply.clone().into(), c.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get::<String>("", "content")
    .unwrap();
    assert_eq!(posted, "Two deliveries are late.");
    let after = task(&db, "Daily delivery review").await;
    assert_eq!(after["runs"], 2);
    assert_eq!(
        after["next"]["kind"], "repeat",
        "the next run is booked from the rule"
    );
}

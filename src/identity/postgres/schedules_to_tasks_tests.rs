//! Schema 147: every schedule that is not a Home panel update becomes a task, keeps
//! its earlier runs, and from then on runs through the task engine in its own work
//! room, posting where it posted before. Nothing runs as a schedule any more.
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

async fn task(db: &PgStore, schedule: &str) -> Value {
    query(&db.db, "SELECT to_jsonb(t)||jsonb_build_object('next',(SELECT jsonb_build_object('kind',k.kind,'due_ms',k.due_ms) FROM relay_task_wakeups k WHERE k.workspace=t.workspace AND k.task=t.id AND k.state='pending'),
          'room_work',(SELECT c.work FROM relay_chats c WHERE c.workspace=t.workspace AND c.id=COALESCE(t.room,t.chat)),
          'room_people',(SELECT jsonb_agg(p.user_id) FROM relay_chat_people p WHERE p.workspace=t.workspace AND p.chat=COALESCE(t.room,t.chat)),
          'room_agents',(SELECT jsonb_agg(x.agent) FROM relay_chat_agents x WHERE x.workspace=t.workspace AND x.chat=COALESCE(t.room,t.chat))) AS v
        FROM relay_tasks t WHERE t.schedule=$1", vec![schedule.into()])
        .await.unwrap().map(|r| r.try_get("", "v").unwrap()).unwrap_or(Value::Null)
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn schema_147_turns_every_schedule_into_a_task_that_runs_and_posts_as_before() {
    let url = std::env::var("RELAY_TEST_DATABASE_URL").unwrap();
    let admin = PgStore::connect(&url).await.unwrap();
    let schema = format!("relay_schedules_to_tasks_{}", uuid::Uuid::new_v4().simple());
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
    let weekly = schedule(
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
    let two_hours = schedule(
        &db,
        &w,
        &a,
        &owner.id,
        old("Stock check", None, Some(7_200), "enabled", Some(soon), ""),
    )
    .await;
    let ninety = schedule(
        &db,
        &w,
        &a,
        &owner.id,
        old("Queue check", None, Some(5_400), "enabled", Some(soon), ""),
    )
    .await;
    let long = schedule(
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
    let once = schedule(
        &db,
        &w,
        &a,
        &owner.id,
        old("Year end note", None, None, "enabled", Some(soon), ""),
    )
    .await;
    let paused = schedule(
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
    let blocked = schedule(
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
    let completed = schedule(
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
    let room = super::super::work_rooms::open(
        &tx,
        &w,
        "Daily delivery review",
        "Runs of a schedule",
        &owner.id,
        &a,
    )
    .await
    .unwrap();
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
        content: "Scheduled work · Daily delivery review\nDo the daily delivery review.".into(),
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

    assert!(db.verify_schema().await.is_err());
    db.migrate().await.unwrap();
    db.migrate().await.unwrap();
    db.verify_schema().await.unwrap();

    // Rules: whole days daily, whole weeks on the first run's weekday and time,
    // whole hours that divide a day at the same minute, anything else in minutes.
    let rules = [
        (&daily, json!({"unit":"day","every":1,"at":"09:30"})),
        (
            &weekly,
            json!({"unit":"week","every":1,"days":[1],"at":"09:30"}),
        ),
        (&two_hours, json!({"unit":"hour","every":2,"from":"01:30"})),
        (&ninety, json!({"unit":"minute","every":90})),
        (&long, json!({"unit":"minute","every":2160})),
        (&once, Value::Null),
    ];
    for (id, rule) in rules {
        let t = task(&db, id).await;
        assert_eq!(t["repeat"], rule, "{t}");
        assert_eq!(
            (t["kind"].as_str(), t["origin"].as_str()),
            (Some("commitment"), Some("schedule"))
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
    // Where it runs and posts: with a post target, the conversation it posted to
    // and its own room; without one, its room only (Activity).
    let d = task(&db, &daily).await;
    assert_eq!(
        (d["chat"].as_str(), d["room"].as_str()),
        (Some(c.as_str()), Some(room.as_str()))
    );
    assert_eq!(
        (d["title"].as_str(), d["runs"].as_i64()),
        (Some("Daily delivery review"), Some(1))
    );
    let wk = task(&db, &weekly).await;
    assert!(wk["room"].is_null());
    assert_eq!(wk["room_work"], true);
    assert_eq!(
        (wk["room_people"].clone(), wk["room_agents"].clone()),
        (json!([owner.id]), json!([a]))
    );
    assert_eq!(wk["runs"], 0);
    // Paused and blocked become paused tasks with their reason; completed, done.
    let p = task(&db, &paused).await;
    assert_eq!(
        (p["paused_reason"].as_str(), p["next"].is_null()),
        (Some("Paused before it became a task."), true)
    );
    let b = task(&db, &blocked).await;
    assert_eq!(
        (b["paused_reason"].as_str(), b["next"].is_null()),
        (Some("Routine access changed."), true)
    );
    assert!(b["room"].as_str().is_some() && b["chat"] == c.as_str());
    let done = task(&db, &completed).await;
    assert_eq!(done["outcome"], "done");
    assert!(done["closed_ms"].as_i64().is_some() && done["next"].is_null());
    // A Home panel's schedule stays as schema 146 left it.
    assert!(task(&db, &home).await.is_null());

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
    let details = db.activity_request(&owner, &w, &ran.reply).await.unwrap();
    assert_eq!(details["schedule"]["task"], d["id"]);
    let calendar = db
        .activity_calendar(&owner, &w, FIRST - 86_400_000, FIRST + 86_400_000, None)
        .await
        .unwrap();
    assert!(
        calendar["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["kind"] == "run"
                && i["task"] == d["id"]
                && i["reply"] == ran.reply.as_str()
                && i["state"] == "done"),
        "{calendar}"
    );

    // Nothing runs as a schedule any more.
    let left = query(&db.db, "SELECT count(*) FILTER (WHERE state<>'completed' OR task IS NULL OR detail<>'Moved to a task.') AS n FROM relay_agent_schedules WHERE id<>$1", vec![home.clone().into()])
        .await.unwrap().unwrap().try_get::<i64>("", "n").unwrap();
    assert_eq!(left, 0);
    assert_eq!(
        query(
            &db.db,
            "SELECT count(*) AS n FROM relay_agent_schedules WHERE state='enabled'",
            vec![]
        )
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "n")
        .unwrap(),
        0
    );

    // Its next run starts through the task engine in its room and posts its result
    // where the schedule posted.
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
    assert_eq!(run.chat, room);
    assert!(
        run.prompt
            .contains("What to do each run: Do the daily delivery review."),
        "{}",
        run.prompt
    );
    db.chat_finish(&run, "Two deliveries are late.", "complete")
        .await
        .unwrap();
    let posted = query(&db.db, "SELECT m.content FROM relay_run_posts p JOIN relay_chat_messages m ON m.workspace=p.workspace AND m.id=p.message WHERE p.reply=$1 AND p.chat=$2",
        vec![reply.clone().into(), c.clone().into()]).await.unwrap().unwrap().try_get::<String>("", "content").unwrap();
    assert!(
        posted.starts_with("**Daily delivery review**")
            && posted.contains("Two deliveries are late."),
        "{posted}"
    );
    let after = task(&db, &daily).await;
    assert_eq!(after["runs"], 2);
    assert_eq!(
        after["next"]["kind"], "repeat",
        "the next run is booked from the rule"
    );
}

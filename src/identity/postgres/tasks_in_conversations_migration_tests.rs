//! Schema 151 moves every task and trigger into a conversation (the owner's one
//! rule). A populated schema 150 workspace upgrades: a channel task keeps its
//! channel and loses its work room, an Activity-only task moves to its person's
//! direct chat with the agent (opened when there is none), a task whose person left
//! is paused, triggers lose their Memory setting and run where they posted or in the
//! direct chat, and work rooms, posted results and subject rooms are gone.
use super::super::event_tests::setup_on;
use super::super::tests::*;
use super::super::*;
use super::lifetime_tests::{claim, one};
use sea_orm_migration::MigratorTrait;
use serde_json::json;

/// A work room as schemas 56 to 150 opened one, holding `person` and `agent`.
pub(super) async fn work_room(
    db: &impl ConnectionTrait,
    w: &str,
    name: &str,
    person: &str,
    agent: &str,
) -> String {
    let room = uuid::Uuid::new_v4().to_string();
    execute(db, "INSERT INTO relay_chats(id,workspace,name,creator,kind,description,work) VALUES($1,$2,$3,$4,'direct','Runs of a task',TRUE)",
        vec![room.clone().into(), w.into(), name.into(), person.into()]).await.unwrap();
    execute(
        db,
        "INSERT INTO relay_chat_people(workspace,chat,user_id) VALUES($1,$2,$3)",
        vec![w.into(), room.clone().into(), person.into()],
    )
    .await
    .unwrap();
    execute(
        db,
        "INSERT INTO relay_chat_agents(workspace,chat,agent,joined_sequence) VALUES($1,$2,$3,0)",
        vec![w.into(), room.clone().into(), agent.into()],
    )
    .await
    .unwrap();
    room
}

/// A schema 150 task of `person` in `chat` (with `room`) and a run booked `due_in`
/// milliseconds from now.
async fn old_task(
    db: &PgStore,
    w: &str,
    agent: &str,
    person: &str,
    chat: &str,
    room: Option<&str>,
    due_in: i64,
) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    execute(&db.db, "INSERT INTO relay_tasks(workspace,id,agent,kind,origin,title,requester,chat,room,repeat,anchor_ms,instructions)
        VALUES($1,$2,$3,'commitment','person','Old task',$4,$5,$6,'{\"unit\":\"hour\"}',1,'Review the late deliveries.')",
        vec![w.into(), id.clone().into(), agent.into(), person.into(), chat.into(), room.into()]).await.unwrap();
    execute(&db.db, "INSERT INTO relay_task_wakeups(workspace,id,task,due_ms,kind) VALUES($1,$2,$3,(extract(epoch FROM clock_timestamp())*1000)::bigint+$4,'repeat')",
        vec![w.into(), uuid::Uuid::new_v4().to_string().into(), id.clone().into(), due_in.into()]).await.unwrap();
    id
}

async fn row(db: &PgStore, sql: &str, id: &str) -> Value {
    one(db, sql, vec![id.into()]).await
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn schema_151_runs_every_task_and_trigger_in_a_conversation() {
    let url = std::env::var("RELAY_TEST_DATABASE_URL").unwrap();
    let admin = PgStore::connect(&url).await.unwrap();
    let schema = format!("relay_tasks_in_chats_{}", uuid::Uuid::new_v4().simple());
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
    super::super::migration::Migrator::up(&db.db, Some(150))
        .await
        .unwrap();
    let mut s = setup_on(db).await;
    let (w, a, c, owner) = (s.w.clone(), s.a.clone(), s.c.clone(), s.owner.clone());
    let colleague = user(&s.db, "tasks-in-chats-colleague").await;
    join(&s.db, &owner, &w, &colleague, "Member").await;
    let gone = user(&s.db, "tasks-in-chats-gone").await;

    // A channel task working in its room, due now.
    let room = work_room(&s.db.db, &w, "Delivery review", &owner.id, &a).await;
    let channel = old_task(&s.db, &w, &a, &owner.id, &c, Some(&room), -1000).await;
    // Activity-only tasks: the owner's with a request queued in its room, a
    // colleague's (no direct chat with the agent yet), and one whose person left.
    let owners_room = work_room(&s.db.db, &w, "Weekly summary", &owner.id, &a).await;
    let activity = old_task(&s.db, &w, &a, &owner.id, &owners_room, None, 3_600_000).await;
    let tx = s.db.workspace_tx(&w).await.unwrap();
    let queued =
        s.db.background_in(
            &tx,
            &owner,
            &w,
            &owners_room,
            &message("Task run · Weekly summary", Some(&a)),
        )
        .await
        .unwrap()
        .unwrap();
    tx.commit().await.unwrap();
    let colleagues_room = work_room(&s.db.db, &w, "Stock", &colleague.id, &a).await;
    let colleagues = old_task(
        &s.db,
        &w,
        &a,
        &colleague.id,
        &colleagues_room,
        None,
        3_600_000,
    )
    .await;
    let orphan = old_task(&s.db, &w, &a, &gone.id, &colleagues_room, None, 3_600_000).await;
    // A remembering trigger posting to the channel, and a fresh one posting
    // nowhere, as schema 150 kept them.
    s.input.chat = Some(c.clone());
    s.save().await;
    let remembering = s.input.id.clone();
    s.input.id = uuid::Uuid::new_v4().to_string();
    s.input.name = "Private order review".into();
    s.input.chat = None;
    s.save().await;
    let fresh = s.input.id.clone();
    s.db.db
        .execute_unprepared(
            "ALTER TABLE relay_event_triggers DISABLE TRIGGER relay_event_trigger_immutable",
        )
        .await
        .unwrap();
    execute(&s.db.db, "UPDATE relay_event_triggers t SET memory='remember',room=x.room,input=t.input||'{\"memory\":\"remember\"}' FROM relay_tasks x WHERE x.id=t.task AND t.id=$1",
        vec![remembering.clone().into()]).await.unwrap();
    execute(&s.db.db, "UPDATE relay_event_triggers SET chat=NULL,input=input||'{\"memory\":\"fresh\"}' WHERE id=$1", vec![fresh.clone().into()]).await.unwrap();
    execute(&s.db.db, "UPDATE relay_tasks t SET chat=t.room,room=NULL FROM relay_event_triggers x WHERE x.task=t.id AND x.id=$1", vec![fresh.clone().into()]).await.unwrap();
    s.db.db
        .execute_unprepared(
            "ALTER TABLE relay_event_triggers ENABLE TRIGGER relay_event_trigger_immutable",
        )
        .await
        .unwrap();

    assert!(s.db.verify_schema().await.is_err());
    s.db.migrate().await.unwrap();
    s.db.migrate().await.unwrap();
    s.db.verify_schema().await.unwrap();

    let task = |id: String| {
        let db = &s.db;
        async move {
            row(db, "SELECT to_jsonb(t)||jsonb_build_object('place',(SELECT jsonb_build_object('kind',c.kind,'keyed',c.direct_key IS NOT NULL,
                'people',(SELECT jsonb_agg(p.user_id) FROM relay_chat_people p WHERE p.workspace=c.workspace AND p.chat=c.id),
                'agents',(SELECT jsonb_agg(x.agent) FROM relay_chat_agents x WHERE x.workspace=c.workspace AND x.chat=c.id)) FROM relay_chats c WHERE c.workspace=t.workspace AND c.id=t.chat),
                'pending',(SELECT count(*) FROM relay_task_wakeups k WHERE k.workspace=t.workspace AND k.task=t.id AND k.state='pending')) AS v FROM relay_tasks t WHERE t.id=$1", &id).await["v"].clone()
        }
    };
    // The channel task stays in the channel; nothing about a room is left.
    let ch = task(channel.clone()).await;
    assert_eq!(ch["chat"], c.as_str());
    assert!(ch.get("room").is_none(), "{ch}");
    // Activity-only tasks run in their person's direct chat with the agent.
    let direct = json!({"kind":"direct","keyed":true,"people":[owner.id],"agents":[a]});
    let act = task(activity.clone()).await;
    assert_eq!(act["place"], direct);
    let col = task(colleagues.clone()).await;
    assert_eq!(
        col["place"],
        json!({"kind":"direct","keyed":true,"people":[colleague.id],"agents":[a]}),
        "opened for the colleague"
    );
    assert_eq!(col["pending"], 1);
    let left = task(orphan.clone()).await;
    assert!(
        left["paused_reason"]
            .as_str()
            .unwrap()
            .contains("no longer in the workspace"),
        "{left}"
    );
    assert_eq!(left["pending"], 0);

    // Triggers lose Memory and run where they posted, or in the direct chat.
    let trigger = |id: String| {
        let db = &s.db;
        async move {
            row(db, "SELECT to_jsonb(t)||jsonb_build_object('task_chat',(SELECT x.chat FROM relay_tasks x WHERE x.id=t.task)) AS v FROM relay_event_triggers t WHERE t.id=$1", &id).await["v"].clone()
        }
    };
    let r = trigger(remembering.clone()).await;
    assert_eq!(
        (r["chat"].as_str(), r["task_chat"].as_str()),
        (Some(c.as_str()), Some(c.as_str()))
    );
    assert!(
        r.get("memory").is_none() && r.get("room").is_none() && r["input"].get("memory").is_none(),
        "{r}"
    );
    let f = trigger(fresh.clone()).await;
    assert_eq!(
        f["chat"], act["chat"],
        "the owner's direct chat with the agent"
    );
    assert_eq!(f["task_chat"], act["chat"]);
    assert!(f["input"].get("memory").is_none());
    assert_eq!(f["state"], "enabled");

    // Work rooms, posted results and subject rooms are gone; nobody is left in a room.
    let gone_objects = one(&s.db, "SELECT to_regclass('relay_run_posts') IS NULL AND to_regclass('relay_subject_rooms') IS NULL
        AND NOT EXISTS(SELECT 1 FROM information_schema.columns WHERE table_schema=current_schema() AND ((table_name='relay_chats' AND column_name='work')
          OR (table_name='relay_tasks' AND column_name='room') OR (table_name='relay_event_deliveries' AND column_name='room')
          OR (table_name='relay_event_triggers' AND column_name IN ('room','memory')))) AS gone,
        (SELECT is_nullable FROM information_schema.columns WHERE table_schema=current_schema() AND table_name='relay_event_triggers' AND column_name='chat') AS nullable", vec![]).await;
    assert_eq!(gone_objects, json!({"gone":true,"nullable":"NO"}));
    for old in [&room, &owners_room, &colleagues_room] {
        let left = row(&s.db, "SELECT (SELECT count(*) FROM relay_chat_people WHERE chat=$1)+(SELECT count(*) FROM relay_chat_agents WHERE chat=$1) AS n", old).await;
        assert_eq!(left["n"], 0);
    }
    let cancelled = row(&s.db, "SELECT m.state,j.state AS job FROM relay_chat_messages m JOIN relay_chat_jobs j ON j.reply=m.id WHERE m.id=$1", &queued.reply).await;
    assert_eq!(cancelled, json!({"state":"cancelled","job":"finished"}));
    assert!(
        !s.db.chats(&owner, &w).await.unwrap()["chats"]
            .as_array()
            .unwrap()
            .iter()
            .any(|chat| chat["id"] == owners_room.as_str()),
        "an old room shows nowhere"
    );

    // They run there now: the channel task in the channel, the trigger in the
    // direct chat, each as its person.
    assert_eq!(s.db.dispatch_task_wakeups().await.unwrap(), 1);
    let reply = row(
        &s.db,
        "SELECT reply FROM relay_task_wakeups WHERE task=$1 AND state='started'",
        &channel,
    )
    .await["reply"]
        .as_str()
        .unwrap()
        .to_owned();
    let run = claim(&s.db, &reply).await;
    assert_eq!(
        (run.chat.as_str(), run.requester.as_str()),
        (c.as_str(), owner.id.as_str())
    );
    s.accept("after-upgrade", "delayed").await;
    assert_eq!(s.db.dispatch_events().await.unwrap(), 2);
    let ran = row(&s.db, "SELECT j.payload->>'chat' AS chat,j.payload->>'requester' AS requester FROM relay_event_deliveries d JOIN relay_chat_jobs j ON j.workspace=d.workspace AND j.reply=d.reply WHERE d.trigger_id=$1", &fresh).await;
    assert_eq!(ran, json!({"chat":act["chat"],"requester":owner.id}));
}

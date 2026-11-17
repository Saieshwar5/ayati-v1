//! Every member's assistant (schema 127): one each, made once, private to its
//! owner, with no other owners, no email address and no place in a channel.
use super::super::{tests::*, *};
use super::NAME;
use crate::identity::{
    agents::config::{SaveAgent, Settings},
    chat::model::NewChat,
};
use serde_json::json;

async fn made(db: &PgStore, who: &User, w: &str) -> String {
    db.ensure_assistant(who, w, "fixture", "openai")
        .await
        .unwrap()
}
async fn count(db: &PgStore, sql: &str, args: Vec<sea_orm::Value>) -> i64 {
    query(&db.db, &format!("SELECT count(*) AS n FROM {sql}"), args)
        .await
        .unwrap()
        .unwrap()
        .try_get("", "n")
        .unwrap()
}
fn profile(id: &str, name: &str, audience: &str, revision: i64) -> SaveAgent {
    SaveAgent {
        id: id.into(),
        name: name.into(),
        role: "Your assistant".into(),
        model: "fixture".into(),
        audience: audience.into(),
        people: None,
        revision,
        settings: Settings {
            provider: "openai".into(),
            ..Default::default()
        },
    }
}

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn each_member_gets_one_private_assistant_and_a_chat_with_it() {
    let db = fixture().await;
    let ravi = user(&db, "ravi").await;
    let priya = user(&db, "priya").await;
    let w = workspace(&db, &ravi).await;
    join(&db, &ravi, &w, &priya, "Member").await;
    assert_eq!(db.assistant_of(&ravi, &w).await.unwrap(), None);

    // Two requests at once, and a later one, agree on one assistant.
    let (a, b) = tokio::join!(
        db.ensure_assistant(&ravi, &w, "fixture", "openai"),
        db.ensure_assistant(&ravi, &w, "fixture", "openai")
    );
    let mine = a.unwrap();
    assert_eq!(b.unwrap(), mine);
    assert_eq!(made(&db, &ravi, &w).await, mine);
    assert_eq!(
        db.assistant_of(&ravi, &w).await.unwrap(),
        Some(mine.clone())
    );
    assert_eq!(
        count(
            &db,
            "relay_shared_agents WHERE workspace=$1",
            vec![w.clone().into()]
        )
        .await,
        1
    );
    let theirs = made(&db, &priya, &w).await;
    assert_ne!(theirs, mine);

    // It is named Ayati, private to its owner, and they have one chat with it.
    let agents = db.agent_profiles(&ravi, &w).await.unwrap();
    let own = agents.iter().find(|a| a["id"] == mine.as_str()).unwrap();
    assert_eq!(own["name"], NAME);
    assert_eq!(own["audience"], "only_me");
    assert_eq!(own["assistant"], true);
    assert_eq!(own["is_owner"], true);
    let chats = db.chats(&ravi, &w).await.unwrap();
    let with_it: Vec<_> = chats["chats"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["agents"] == json!([mine]))
        .collect();
    assert_eq!(with_it.len(), 1);
    assert_eq!(with_it[0]["kind"], "direct");
    // Priya can't talk to Ravi's assistant, and it is not in her directory.
    assert!(!db.agent_access(&priya.id, &w, &mine).await.unwrap());
    let seen = db.allowed_shared_agents(&priya, &w).await.unwrap();
    assert!(seen.iter().all(|a| a["id"] != mine.as_str()));
    assert!(
        seen.iter()
            .any(|a| a["id"] == theirs.as_str() && a["assistant"] == true)
    );
}

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn an_assistant_keeps_its_fixed_rules() {
    let db = fixture().await;
    let ravi = user(&db, "ravi").await;
    let priya = user(&db, "priya").await;
    let w = workspace(&db, &ravi).await;
    join(&db, &ravi, &w, &priya, "Member").await;
    let mine = made(&db, &ravi, &w).await;

    // Its owner renames it; it can't be opened to other people.
    db.save_agent_profile(&ravi, &w, &profile(&mine, "Kai", "only_me", 1))
        .await
        .unwrap();
    let opened = db
        .save_agent_profile(&ravi, &w, &profile(&mine, "Kai", "everyone", 2))
        .await
        .unwrap_err();
    assert!(opened.to_string().contains("stays private to you"));
    // No other owners.
    let owners = db
        .change_agent_owners(
            &ravi,
            &w,
            &mine,
            &serde_json::from_value(json!({"add": priya.id, "revision": 0})).unwrap(),
        )
        .await
        .unwrap_err();
    assert!(owners.to_string().contains("no other owners"));
    // No email address.
    let email = db
        .email_activate(&ravi, &w, &mine, "mail.example.test")
        .await
        .unwrap_err();
    assert!(email.to_string().contains("no email address"));
    // No channel.
    let channel = db
        .chat_create(
            &ravi,
            &w,
            &NewChat {
                id: uuid::Uuid::new_v4().to_string(),
                name: "ops".into(),
                kind: "channel".into(),
                description: String::new(),
                users: vec![priya.id.clone()],
                agents: vec![mine.clone()],
            },
        )
        .await;
    assert!(channel.is_err());
    // The renamed assistant is still the one the member has.
    assert_eq!(made(&db, &ravi, &w).await, mine);
    assert!(
        db.agent_is_new(&w, &uuid::Uuid::new_v4().to_string())
            .await
            .unwrap()
    );
    assert!(!db.agent_is_new(&w, &mine).await.unwrap());
}

async fn text(db: &PgStore, sql: &str, args: Vec<sea_orm::Value>) -> Option<String> {
    query(&db.db, &format!("SELECT ({sql})::text AS v"), args)
        .await
        .unwrap()
        .unwrap()
        .try_get("", "v")
        .unwrap()
}

/// The queued job in `chat`, claimed as an executor would.
async fn queued(db: &PgStore, chat: &str) -> crate::identity::chat::model::Job {
    let payload: Value = query(
        &db.db,
        "SELECT j.payload FROM relay_chat_jobs j WHERE j.payload->>'chat'=$1 AND j.state='queued'",
        vec![chat.into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "payload")
    .unwrap();
    super::super::handoff_tests::claim(db, &serde_json::from_value(payload).unwrap()).await
}

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn an_assistants_schedule_runs_as_a_thread_in_its_one_room() {
    let mut db = fixture().await;
    db.pooled = true;
    let ravi = user(&db, "ravi").await;
    let w = workspace(&db, &ravi).await;
    let mine = made(&db, &ravi, &w).await;
    let room = text(
        &db,
        "SELECT chat FROM relay_chat_agents WHERE agent=$1",
        vec![mine.clone().into()],
    )
    .await
    .unwrap();
    let mut input = super::super::automation_tests::schedule(&room, Some(3600));
    input.chat = None;
    db.save_schedule(&ravi, &w, &mine, &input).await.unwrap();
    assert_eq!(db.dispatch_schedules_at(input.first_ms).await.unwrap(), 1);

    // No work room: the run is in the room, in a thread under one line.
    assert_eq!(
        count(
            &db,
            "relay_chats WHERE workspace=$1 AND work",
            vec![w.clone().into()]
        )
        .await,
        0
    );
    let thread = text(
        &db,
        "SELECT thread FROM relay_assistant_threads WHERE agent=$1",
        vec![mine.clone().into()],
    )
    .await
    .unwrap();
    let line = db.chat_messages(&ravi, &w, &room).await.unwrap();
    let all = line["messages"].as_array().unwrap();
    let root = all.iter().find(|m| m["id"] == thread.as_str()).unwrap();
    assert_eq!(root["content"], "Schedule: Delivery review");
    assert!(root["thread"].is_null());
    let run: Vec<_> = all
        .iter()
        .filter(|m| m["thread"] == thread.as_str())
        .collect();
    assert_eq!(run.len(), 2, "the run's request and the assistant's reply");
    // The next run joins the same thread.
    let job = queued(&db, &room).await;
    assert_eq!(job.chat, room);
    db.chat_finish(&job, "Nothing late today.", "complete")
        .await
        .unwrap();
    assert_eq!(
        db.dispatch_schedules_at(input.first_ms + 3_600_000)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        count(
            &db,
            "relay_assistant_threads WHERE agent=$1",
            vec![mine.clone().into()]
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &db,
            "relay_chat_messages WHERE chat=$1 AND thread=$2",
            vec![room.clone().into(), thread.clone().into()]
        )
        .await,
        4
    );
    // The second run reads the first one's answer: one room, one memory.
    let next = queued(&db, &room).await;
    assert!(next.prompt.contains("Nothing late today."));
}

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn an_assistant_remembers_what_its_tools_told_it_and_other_agents_do_not() {
    let db = fixture().await;
    let ravi = user(&db, "ravi").await;
    let w = workspace(&db, &ravi).await;
    let mine = made(&db, &ravi, &w).await;
    let shared = uuid::Uuid::new_v4().to_string();
    db.save_agent_profile(&ravi, &w, &profile(&shared, "HR Desk", "everyone", 0))
        .await
        .unwrap();
    db.db
        .execute_unprepared(&format!(
            "INSERT INTO relay_control_systems(workspace,server,approved,revision) VALUES('{w}','mailer',true,1);
             INSERT INTO relay_control_connections(workspace,id,server,definition,fingerprint) VALUES('{w}','mail','mailer','{{\"id\":\"mailer\",\"name\":\"Mail\",\"operations\":[]}}','fp')"
        ))
        .await
        .unwrap();
    for (agent, expected) in [(mine, true), (shared, false)] {
        let chat = db
            .chat_create(
                &ravi,
                &w,
                &NewChat {
                    id: uuid::Uuid::new_v4().to_string(),
                    name: "Direct".into(),
                    kind: "direct".into(),
                    description: String::new(),
                    users: vec![],
                    agents: vec![agent.clone()],
                },
            )
            .await
            .unwrap();
        let job = db
            .chat_send(
                &ravi,
                &w,
                &chat,
                &message("Read my payslip mail", Some(&agent)),
            )
            .await
            .unwrap()
            .unwrap();
        // The reply was built from a tool call.
        execute(&db.db, "INSERT INTO relay_control_actions(id,workspace,agent,chat,job,requester,request_id,connection,operation,arguments,title,effect,rule_revision,connection_revision,approver,state,created,updated,run_cycle,requires_approval) VALUES($1,$2,$3,$4,$5,$6,'r','mail','mail.read','{}','Read mail','read',1,1,$6,'completed',1,1,0,false)",
            vec![uuid::Uuid::new_v4().to_string().into(), w.clone().into(), agent.clone().into(), chat.clone().into(), job.reply.clone().into(), ravi.id.clone().into()]).await.unwrap();
        let running = db.claim_job().await.unwrap().unwrap();
        db.chat_finish(&running, "Your payslip shows 62,400.", "complete")
            .await
            .unwrap();
        let later = db
            .chat_send(
                &ravi,
                &w,
                &chat,
                &message("What did it show?", Some(&agent)),
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            later.prompt.contains("62,400"),
            expected,
            "assistant: {expected}"
        );
    }
}

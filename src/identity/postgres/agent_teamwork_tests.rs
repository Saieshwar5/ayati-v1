//! People and agents working together: roles, who is here, asking a person and
//! waiting for the answer, and agents that don't take work from other agents.
use super::super::{tests::*, *};
use crate::identity::{
    agents::config::{SaveAgent, Settings},
    chat::model::{Job, NewMessage},
    control::broker::invoke,
};
use serde_json::json;

async fn new_agent(
    db: &PgStore,
    owner: &User,
    w: &str,
    name: &str,
    handles: &str,
    helper: bool,
) -> String {
    let input = SaveAgent {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.into(),
        role: format!("{name} assistant"),
        model: "fixture".into(),
        audience: "everyone".into(),
        people: None,
        revision: 0,
        settings: Settings {
            provider: "openai".into(),
            handles: handles.into(),
            helper,
            ..Default::default()
        },
    };
    db.save_agent_profile(owner, w, &input).await.unwrap();
    input.id
}
async fn ask(db: &PgStore, who: &User, w: &str, c: &str, a: &str, text: &str) -> Job {
    let job = db
        .chat_send(who, w, c, &message(text, Some(a)))
        .await
        .unwrap()
        .unwrap();
    super::super::handoff_tests::claim(db, &job).await
}
async fn field(db: &PgStore, sql: &str, id: &str) -> Value {
    json_rows(&db.db, sql, vec![id.into()])
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap_or(Value::Null)
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn agents_know_who_is_here_ask_a_person_and_wait_for_the_answer() {
    let mut db = fixture().await;
    db.pooled = true;
    let meera = user(&db, "team-meera").await;
    let arjun = user(&db, "team-arjun").await;
    let kavya = user(&db, "team-kavya").await;
    let w = workspace(&db, &meera).await;
    join(&db, &meera, &w, &arjun, "Member").await;
    join(&db, &meera, &w, &kavya, "Member").await;
    // Roles: people say what they handle on their profile; agents in settings.
    db.save_member_profile(&arjun, &w, "Arjun Patel", "Logistics", "")
        .await
        .unwrap();
    db.save_member_handles(&arjun, &w, "Couriers, delivery dates")
        .await
        .unwrap();
    assert!(
        db.save_member_handles(&arjun, &w, &"x".repeat(161))
            .await
            .is_err()
    );
    execute(&db.db, "INSERT INTO relay_member_presence(workspace,user_id,status_kind,status_set_ms) VALUES($1,$2,'busy',1)",
        vec![w.clone().into(), arjun.id.clone().into()]).await.unwrap();
    let sales = new_agent(&db, &meera, &w, "Sales", "Quotes and prices", true).await;
    let stock = new_agent(&db, &meera, &w, "Stock", "Stock levels, reservations", true).await;
    let payroll = new_agent(&db, &meera, &w, "Payroll", "Salaries", false).await;
    let mut long = Settings {
        handles: "y".repeat(161),
        ..Default::default()
    };
    assert!(long.validate().is_err());
    long.handles.clear();
    let orders = chat(
        &db,
        &meera,
        &w,
        vec![arjun.id.clone()],
        vec![sales.clone(), stock.clone(), payroll.clone()],
    )
    .await;

    // Who is here: people with roles and status, and the other agents.
    let job = ask(
        &db,
        &meera,
        &w,
        &orders,
        &sales,
        "Quote 40 chairs for Kapoor by Friday",
    )
    .await;
    let purpose = db.conversation_purpose(&job).await.unwrap().unwrap();
    assert!(purpose.contains(&format!("- Arjun Patel (person, ID {}) · Logistics · Handles: Couriers, delivery dates · Status: Busy", arjun.id)), "{purpose}");
    assert!(
        purpose.contains("- Stock (agent) · Stock assistant · Handles: Stock levels, reservations")
    );
    assert!(!purpose.contains("- Sales (agent)"), "not itself");
    // Helpers: an agent whose owners keep it from other agents is left out and refused.
    let helpers = db
        .handoff_helpers(&job)
        .await
        .unwrap()
        .unwrap()
        .instructions();
    assert!(
        helpers.contains("Stock (ID") && helpers.contains("Handles: Stock levels, reservations")
    );
    assert!(!helpers.contains("Payroll"));
    let refused = invoke(
        &db,
        &job,
        "request_handoff",
        json!({"key":"pay","agent":payroll,"task":"Check pay"}),
    )
    .await;
    assert!(refused.is_err());

    // Asking people: only someone here, not the person who asked.
    for (person, why) in [
        (&meera, "ask_question"),
        (&kavya, "not in this conversation"),
    ] {
        let result = invoke(
            &db,
            &job,
            "ask_person",
            json!({"key":"k","person":person.id,"question":"Who?"}),
        )
        .await
        .unwrap();
        assert_eq!(result["state"], "invalid");
        assert!(
            result["message"].as_str().unwrap().contains(why),
            "{result}"
        );
    }
    let asked = invoke(&db, &job, "ask_person", json!({"key":"delivery","person":arjun.id,"question":"Can a courier deliver 32 chairs to Kapoor by Friday?"})).await.unwrap();
    assert_eq!(
        (asked["state"].as_str(), asked["status"].as_str()),
        (Some("pending"), Some("Busy"))
    );
    let again = invoke(&db, &job, "ask_person", json!({"key":"delivery","person":arjun.id,"question":"Can a courier deliver 32 chairs to Kapoor by Friday?"})).await.unwrap();
    assert_eq!(again["id"], asked["id"], "a retry asks once");
    assert_eq!(
        invoke(
            &db,
            &job,
            "ask_person",
            json!({"key":"other","person":arjun.id,"question":"And Monday?"})
        )
        .await
        .unwrap()["state"],
        "invalid",
        "one at a time"
    );
    // The question is posted where the agent answers, @mentioning Arjun.
    let reply_thread = field(
        &db,
        "SELECT to_jsonb(thread) AS data FROM relay_chat_messages WHERE id=$1",
        &job.reply,
    )
    .await;
    let posted = field(&db, "SELECT jsonb_build_object('content',content,'thread',thread,'author',author) AS data FROM relay_chat_messages WHERE update_of=$1", &job.reply).await;
    assert_eq!(
        posted["content"],
        format!(
            "[@Arjun Patel](mention:user:{}) Can a courier deliver 32 chairs to Kapoor by Friday?",
            arjun.id
        )
    );
    assert_eq!(
        (&posted["thread"], posted["author"].as_str()),
        (&reply_thread, Some(sales.as_str()))
    );
    // The attempt finishes and the task waits for Arjun.
    db.chat_finish(&job, "I've asked Arjun about delivery.", "complete")
        .await
        .unwrap();
    let state = |db: &PgStore| {
        let (db, reply) = (db.clone(), job.reply.clone());
        async move {
            field(&db, "SELECT jsonb_build_object('job',j.state,'phase',m.progress->>'state') AS data FROM relay_chat_jobs j JOIN relay_chat_messages m ON m.id=j.reply WHERE j.reply=$1", &reply).await
        }
    };
    assert_eq!(state(&db).await, json!({"job":"waiting","phase":"person"}));

    // Arjun answers in the thread. Even if his browser sends it to Sales (the
    // agent Meera asked), it answers the waiting task and starts no new work.
    let jobs = |db: &PgStore| {
        let (db, w) = (db.clone(), w.clone());
        async move {
            field(
                &db,
                "SELECT to_jsonb(count(*)) AS data FROM relay_chat_jobs WHERE workspace=$1",
                &w,
            )
            .await
        }
    };
    let before = jobs(&db).await;
    let answer = NewMessage {
        thread: reply_thread.as_str().map(str::to_owned),
        agents: vec![sales.clone()],
        ..message("Yes, Thursday.", None)
    };
    db.chat_send(&arjun, &w, &orders, &answer).await.unwrap();
    assert_eq!(jobs(&db).await, before);
    assert_eq!(state(&db).await["job"], "queued", "the task continues");
    let resumed = super::super::handoff_tests::claim(&db, &job).await;
    let saved: Value =
        serde_json::from_str(&db.task_continuation(&resumed).await.unwrap().unwrap()).unwrap();
    assert_eq!(
        saved["people_asked"],
        json!([{"key":"delivery","person":"Arjun Patel","question":"Can a courier deliver 32 chairs to Kapoor by Friday?","state":"answered","answer":"Yes, Thursday."}])
    );
    // Naming the agent still asks it.
    let named = NewMessage {
        thread: reply_thread.as_str().map(str::to_owned),
        ..message(
            &format!("[@Sales](mention:agent:{sales}) also price 10 tables"),
            Some(&sales),
        )
    };
    assert!(
        db.chat_send(&arjun, &w, &orders, &named)
            .await
            .unwrap()
            .is_some()
    );

    // Nobody answers: after the wait, the task continues without an answer.
    let second = invoke(
        &db,
        &resumed,
        "ask_person",
        json!({"key":"stock-ok","person":arjun.id,"question":"Is the walnut finish fine?"}),
    )
    .await
    .unwrap();
    assert_eq!(second["state"], "pending");
    db.chat_finish(&resumed, "Waiting for Arjun.", "complete")
        .await
        .unwrap();
    assert_eq!(db.expire_person_asks().await.unwrap(), 0, "not yet");
    execute(
        &db.db,
        "UPDATE relay_person_asks SET deadline_ms=0 WHERE id=$1",
        vec![second["id"].as_str().unwrap().into()],
    )
    .await
    .unwrap();
    assert_eq!(db.expire_person_asks().await.unwrap(), 1);
    assert_eq!(state(&db).await["job"], "queued");
    // A third ask is the most; then the agent must say what is missing.
    let third = super::super::handoff_tests::claim(&db, &resumed).await;
    assert_eq!(
        invoke(
            &db,
            &third,
            "ask_person",
            json!({"key":"three","person":arjun.id,"question":"Last one?"})
        )
        .await
        .unwrap()["state"],
        "pending"
    );
    db.chat_finish(&third, "Waiting.", "complete")
        .await
        .unwrap();
    execute(
        &db.db,
        "UPDATE relay_person_asks SET deadline_ms=0 WHERE state='pending'",
        vec![],
    )
    .await
    .unwrap();
    db.expire_person_asks().await.unwrap();
    let fourth = super::super::handoff_tests::claim(&db, &third).await;
    assert_eq!(
        invoke(
            &db,
            &fourth,
            "ask_person",
            json!({"key":"four","person":arjun.id,"question":"Another?"})
        )
        .await
        .unwrap()["state"],
        "invalid"
    );
    // Stopping the task stops any wait.
    db.chat_finish(&fourth, "Done.", "complete").await.unwrap();

    // A direct chat has nobody else to ask.
    let direct = db
        .chat_create(
            &meera,
            &w,
            &crate::identity::chat::model::NewChat {
                id: uuid::Uuid::new_v4().to_string(),
                name: "Direct".into(),
                kind: "direct".into(),
                description: String::new(),
                users: vec![],
                agents: vec![sales.clone()],
            },
        )
        .await
        .unwrap();
    let private = ask(&db, &meera, &w, &direct, &sales, "Hello").await;
    assert_eq!(
        invoke(
            &db,
            &private,
            "ask_person",
            json!({"key":"k","person":arjun.id,"question":"Hi?"})
        )
        .await
        .unwrap()["state"],
        "invalid"
    );
}

//! A member's assistant helps set up an agent (schema 139).
use super::super::super::{tests::*, *};
use super::SetupRequest;
use crate::identity::agents::config::{SaveAgent, Settings};
use crate::identity::chat::model::Job;
use serde_json::json;

async fn one(db: &PgStore, sql: &str, args: Vec<sea_orm::Value>) -> Value {
    query(&db.db, &format!("SELECT to_jsonb(({sql})) AS v"), args)
        .await
        .unwrap()
        .unwrap()
        .try_get("", "v")
        .unwrap()
}
async fn claimed(db: &PgStore, job: &Job) -> Job {
    super::super::super::handoff_tests::claim(db, job).await
}
/// One turn of the setup chat for the agent called `name`.
fn turn(name: &str, agent: Option<&str>, said: &str) -> SetupRequest {
    serde_json::from_value(json!({
        "id": uuid::Uuid::new_v4().to_string(),
        "setup": {
            "name": name, "role": "", "agent": agent, "ground_rules": "",
            "skills": [{"name": "returns", "description": "Use when a customer returns an item.", "steps": "1. Ask why."}],
            "messages": [
                {"role": "person", "text": "It answers customers."},
                {"role": "agent", "text": "Who takes over?"},
                {"role": "person", "text": said},
            ],
        },
    }))
    .unwrap()
}
fn part(fields: Value) -> super::Part {
    serde_json::from_value(fields).unwrap()
}
async fn hand_back(db: &PgStore, job: &Job, fields: Value) -> String {
    db.write_agent_setup(job, &part(fields)).await.unwrap()["state"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn an_assistant_drafts_an_agents_setup_and_nothing_is_saved() {
    let mut db = fixture().await;
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
    let room = one(
        &db,
        "SELECT chat FROM relay_chat_agents WHERE agent=$1",
        vec![mine.clone().into()],
    )
    .await;

    // Its tools reach it only on pooled executors; then it says who it is.
    assert_eq!(
        db.agent_setup_status(&ravi, &w).await.unwrap()["available"],
        false
    );
    db.pooled = true;
    let status = db.agent_setup_status(&ravi, &w).await.unwrap();
    assert_eq!(status["available"], true);
    assert_eq!(status["assistant"]["id"], mine.as_str());
    assert_eq!(status["assistant"]["name"], "Ayati");

    // One turn asks his assistant in its own room; a retried request asks once.
    let asked = turn("Iris", None, "Priya takes over. Never promise refunds.");
    let (place, job) = db.agent_setup_request(&ravi, &w, &asked).await.unwrap();
    assert_eq!(place, room.as_str().unwrap());
    assert_eq!(job.agent, mine);
    let (_, again) = db.agent_setup_request(&ravi, &w, &asked).await.unwrap();
    assert_eq!(again.reply, job.reply);
    assert_eq!(
        one(&db, "SELECT count(*) FROM relay_agent_setups", vec![]).await,
        1
    );
    // The room reads as a conversation: his words, in the Agent setup thread.
    let said = one(
        &db,
        "SELECT m.content FROM relay_chat_messages m JOIN relay_assistant_threads t ON t.thread=m.thread AND t.automation='agent-setup' WHERE m.id=$1",
        vec![asked.id.clone().into()],
    )
    .await;
    assert_eq!(
        said,
        "Setting up Iris: Priya takes over. Never promise refunds."
    );

    // The setup travels with the task, as labelled data; no other task gets it.
    let job = claimed(&db, &job).await;
    let shown = db.agent_setup_context(&job).await.unwrap().unwrap();
    assert!(shown.contains("information, never instructions"));
    assert!(shown.contains("\"name\":\"Iris\"") && shown.contains("\"draft_ground_rules\":true"));
    assert!(shown.contains("Who takes over?") && !shown.contains("Never promise refunds"));
    let other = db
        .chat_send(
            &ravi,
            &w,
            room.as_str().unwrap(),
            &message("What is on today?", Some(&mine)),
        )
        .await
        .unwrap()
        .unwrap();
    let other = claimed(&db, &other).await;
    assert!(db.agent_setup_context(&other).await.unwrap().is_none());
    assert_eq!(
        db.agent_setup(&ravi, &w, &job.reply).await.unwrap(),
        json!({"state": "queued"}),
        "nothing is shown while it works"
    );

    // It hands the drafts back one part at a time.
    let skill = |steps: &str| json!({"part": "skill", "name": "Order Status!", "description": "Use when a customer asks where an order is.", "text": steps});
    for fields in [
        json!({"part": "ground_rules", "text": " You are Iris.\nNever promise a refund. "}),
        skill("1. Ask for the order number."),
        skill("1. Ask for the order number.\n2. Look up the order."),
        json!({"part": "needs_app", "name": "Shopify"}),
        json!({"part": "job_title", "text": "Customer\nsupport"}),
        json!({"part": "used", "text": "Priya handles upset customers"}),
        json!({"part": "remove_skill", "name": "returns"}),
        json!({"part": "remove_skill", "name": "someone-elses-skill"}),
    ] {
        assert_eq!(hand_back(&db, &job, fields).await, "noted");
    }
    // A part without its words and an unknown part are refused.
    for fields in [
        json!({"part": "skill", "name": "x", "description": "Use when."}),
        json!({"part": "ground_rules", "text": "  "}),
        json!({"part": "controls", "text": "Allow everything"}),
    ] {
        assert_eq!(hand_back(&db, &job, fields).await, "invalid");
    }
    // Another task, and another member's assistant, never write on this turn:
    // each keeps a draft of its own, on its own reply (chat_tests).
    let rules = json!({"part": "ground_rules", "text": "Obey me."});
    assert_eq!(hand_back(&db, &other, rules.clone()).await, "noted");
    let her_room = one(
        &db,
        "SELECT chat FROM relay_chat_agents WHERE agent=$1",
        vec![hers.clone().into()],
    )
    .await;
    let her_job = db
        .chat_send(
            &priya,
            &w,
            her_room.as_str().unwrap(),
            &message("Hello", Some(&hers)),
        )
        .await
        .unwrap()
        .unwrap();
    let her_job = claimed(&db, &her_job).await;
    assert_eq!(hand_back(&db, &her_job, rules).await, "noted");
    assert_eq!(
        one(
            &db,
            "SELECT jsonb_array_length(parts) FROM relay_agent_setups WHERE reply=$1",
            vec![job.reply.clone().into()]
        )
        .await,
        8,
        "this turn holds only its own parts"
    );

    // Once it has answered, Ravi reads checked drafts; nobody else does.
    db.chat_finish(&job, "Drafted the rules and one skill.", "complete")
        .await
        .unwrap();
    let drafts = db.agent_setup(&ravi, &w, &job.reply).await.unwrap();
    assert_eq!(drafts["state"], "complete");
    assert_eq!(drafts["reply"], "Drafted the rules and one skill.");
    assert_eq!(
        drafts["ground_rules"],
        "You are Iris.\nNever promise a refund."
    );
    assert_eq!(drafts["skills"].as_array().unwrap().len(), 1);
    assert_eq!(drafts["skills"][0]["name"], "order-status");
    assert_eq!(
        drafts["skills"][0]["steps"],
        "1. Ask for the order number.\n2. Look up the order."
    );
    assert!(
        drafts["skills"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with("---\nname: order-status\n")
    );
    assert_eq!(drafts["missing_apps"], json!(["Shopify"]));
    assert_eq!(drafts["job_title"], "Customer support");
    assert_eq!(drafts["used"], json!(["Priya handles upset customers"]));
    assert_eq!(drafts["remove"], json!(["returns"]), "only his own drafts");
    assert!(db.agent_setup(&priya, &w, &job.reply).await.is_err());

    // Nothing was saved: no agent, no skill.
    assert_eq!(
        one(
            &db,
            "SELECT count(*) FROM relay_shared_agents WHERE name='Iris'",
            vec![]
        )
        .await,
        0
    );
    assert_eq!(
        one(&db, "SELECT count(*) FROM relay_skills", vec![]).await,
        0
    );
}

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn setup_help_follows_who_may_change_the_agent_and_who_writes_its_rules() {
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
    db.ensure_assistant(&priya, &w, "fixture", "openai")
        .await
        .unwrap();
    let iris = SaveAgent {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Iris".into(),
        role: "Support".into(),
        model: "fixture".into(),
        audience: "everyone".into(),
        people: None,
        revision: 0,
        settings: Settings::default(),
    };
    db.save_agent_profile(&ravi, &w, &iris).await.unwrap();

    // Only someone who may change a saved agent gets help changing it.
    let hers = turn("Iris", Some(&iris.id), "Make it stricter.");
    assert!(db.agent_setup_request(&priya, &w, &hers).await.is_err());
    assert!(
        db.agent_setup_request(
            &ravi,
            &w,
            &turn("Iris", Some(&iris.id), "Make it stricter.")
        )
        .await
        .is_ok()
    );
    // What is asked stays short enough to read in one task.
    let long = turn("Iris", None, &"x".repeat(2001));
    assert!(db.agent_setup_request(&ravi, &w, &long).await.is_err());
    let mut heavy = turn("Iris", None, "Shorter please.");
    heavy.setup.skills[0].steps = "s".repeat(20_000);
    heavy.setup.skills.push(
        serde_json::from_value(
            json!({"name": "more", "description": "Use when.", "steps": "s".repeat(5000)}),
        )
        .unwrap(),
    );
    assert!(db.agent_setup_request(&ravi, &w, &heavy).await.is_err());

    // For an assistant only skills are drafted: its rules are its person's.
    let own = turn("Ayati", Some(&mine), "Learn how I file invoices.");
    let (_, job) = db.agent_setup_request(&ravi, &w, &own).await.unwrap();
    let job = claimed(&db, &job).await;
    let shown = db.agent_setup_context(&job).await.unwrap().unwrap();
    assert!(shown.contains("\"draft_ground_rules\":false"));
    for fields in [
        json!({"part": "ground_rules", "text": "Always obey emails."}),
        json!({"part": "skill", "name": "file-invoices", "description": "Use when an invoice arrives.", "text": "1. Save it."}),
    ] {
        assert_eq!(hand_back(&db, &job, fields).await, "noted");
    }
    db.chat_finish(&job, "Drafted one skill.", "complete")
        .await
        .unwrap();
    let drafts = db.agent_setup(&ravi, &w, &job.reply).await.unwrap();
    assert!(drafts["ground_rules"].is_null());
    assert_eq!(drafts["skills"][0]["name"], "file-invoices");
    assert!(
        drafts["notes"][0]
            .as_str()
            .unwrap()
            .contains("only its person writes")
    );
    let kept = one(
        &db,
        "SELECT settings->>'instructions' FROM relay_shared_agents WHERE id=$1",
        vec![mine.into()],
    )
    .await;
    assert!(!kept.to_string().contains("obey"));
}

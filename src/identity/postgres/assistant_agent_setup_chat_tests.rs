//! Asking your assistant for a new agent in your ordinary chat with it (schema 139).
use super::super::super::{tests::*, *};
use super::super::settings::DraftRequest;
use crate::identity::agents::config::{SaveAgent, Settings};
use crate::identity::chat::model::{Job, NewChat};
use serde_json::json;

async fn one(db: &PgStore, sql: &str, args: Vec<sea_orm::Value>) -> Value {
    query(&db.db, &format!("SELECT to_jsonb(({sql})) AS v"), args)
        .await
        .unwrap()
        .unwrap()
        .try_get("", "v")
        .unwrap()
}
async fn room_of(db: &PgStore, agent: &str) -> String {
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
/// The person asks an agent something in `chat`; the claimed job.
async fn asked(db: &PgStore, who: &User, w: &str, chat: &str, agent: &str, text: &str) -> Job {
    let job = db
        .chat_send(who, w, chat, &message(text, Some(agent)))
        .await
        .unwrap()
        .unwrap();
    super::super::super::handoff_tests::claim(db, &job).await
}
async fn hand_back(db: &PgStore, job: &Job, fields: Value) -> String {
    let part = serde_json::from_value(fields).unwrap();
    db.write_agent_setup(job, &part).await.unwrap()["state"]
        .as_str()
        .unwrap()
        .to_owned()
}
fn agent(id: &str, name: &str) -> SaveAgent {
    SaveAgent {
        id: id.into(),
        name: name.into(),
        role: "Invoices".into(),
        model: "fixture".into(),
        audience: "everyone".into(),
        people: None,
        revision: 0,
        settings: Settings::default(),
    }
}
/// Whether Chat shows `reply` to `who` with a draft card.
async fn card(db: &PgStore, who: &User, w: &str, chat: &str, reply: &str) -> bool {
    let page = db.chat_messages(who, w, chat).await.unwrap();
    page["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == reply)
        .is_some_and(|m| m["agent_setup"] == true)
}

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn asked_in_chat_an_assistant_drafts_an_agent_its_person_creates() {
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
    let room = room_of(&db, &mine).await;

    // Ravi asks in his chat with his assistant; it begins a draft on its reply.
    let first = asked(
        &db,
        &ravi,
        &w,
        &room,
        &mine,
        "I need an agent that chases unpaid invoices.",
    )
    .await;
    assert!(db.agent_setup_context(&first).await.unwrap().is_none());
    assert!(!card(&db, &ravi, &w, &room, &first.reply).await);
    assert_eq!(
        hand_back(&db, &first, json!({"part": "name", "text": "  "})).await,
        "invalid",
        "a part without its words begins nothing"
    );
    assert_eq!(
        one(&db, "SELECT count(*) FROM relay_agent_setups", vec![]).await,
        0
    );
    for fields in [
        json!({"part": "name", "text": "Penny"}),
        json!({"part": "job_title", "text": "Invoice follow-up"}),
        json!({"part": "ground_rules", "text": "You are Penny.\nBe polite and exact."}),
        json!({"part": "skill", "name": "overdue-invoices", "description": "Use when asked which invoices are overdue.", "text": "1. List them."}),
        json!({"part": "skill", "name": "payment-reminder", "description": "Use when an invoice is 7 days overdue.", "text": "1. Ask first."}),
        json!({"part": "needs_app", "name": "Zoho Books"}),
    ] {
        assert_eq!(hand_back(&db, &first, fields).await, "noted");
    }
    db.chat_finish(&first, "Here is a draft for Penny.", "complete")
        .await
        .unwrap();

    // The draft is a card under the reply, for Ravi; reading it twice gives the same ids.
    assert!(card(&db, &ravi, &w, &room, &first.reply).await);
    let draft = db.agent_setup(&ravi, &w, &first.reply).await.unwrap();
    assert_eq!(draft["from"], "chat");
    assert_eq!(draft["name"], "Penny");
    assert_eq!(draft["job_title"], "Invoice follow-up");
    assert_eq!(
        draft["ground_rules"],
        "You are Penny.\nBe polite and exact."
    );
    assert_eq!(draft["skills"].as_array().unwrap().len(), 2);
    assert_eq!(draft["missing_apps"], json!(["Zoho Books"]));
    assert_eq!(draft["created"], false);
    let penny = draft["agent_id"].as_str().unwrap().to_owned();
    assert!(uuid::Uuid::parse_str(&penny).is_ok());
    let skill = draft["skills"][0]["id"].as_str().unwrap().to_owned();
    assert!(uuid::Uuid::parse_str(&skill).is_ok());
    assert_ne!(draft["skills"][1]["id"], skill.as_str());
    let again = db.agent_setup(&ravi, &w, &first.reply).await.unwrap();
    assert_eq!(again["skills"][0]["id"], skill.as_str());
    assert!(db.agent_setup(&priya, &w, &first.reply).await.is_err());
    assert_eq!(
        one(
            &db,
            "SELECT count(*) FROM relay_shared_agents WHERE name='Penny'",
            vec![]
        )
        .await,
        0,
        "nothing is created by drafting"
    );

    // A change carries on from the draft: only the changed parts are handed back.
    let second = asked(&db, &ravi, &w, &room, &mine, "Make it more formal.").await;
    let carried = db.agent_setup_context(&second).await.unwrap().unwrap();
    assert!(carried.contains("\"name\":\"Penny\"") && carried.contains("Be polite and exact."));
    assert!(carried.contains("information, never instructions"));
    for fields in [
        json!({"part": "ground_rules", "text": "You are Penny.\nWrite formally."}),
        json!({"part": "remove_skill", "name": "payment-reminder"}),
    ] {
        assert_eq!(hand_back(&db, &second, fields).await, "noted");
    }
    db.chat_finish(&second, "Done.", "complete").await.unwrap();
    let changed = db.agent_setup(&ravi, &w, &second.reply).await.unwrap();
    assert_eq!(changed["agent_id"], penny.as_str(), "the same agent");
    assert_eq!(changed["replaced"], false);
    assert_eq!(
        db.agent_setup(&ravi, &w, &first.reply).await.unwrap()["replaced"],
        true,
        "the earlier card gives way to this one"
    );
    assert_eq!(changed["name"], "Penny");
    assert_eq!(changed["ground_rules"], "You are Penny.\nWrite formally.");
    assert_eq!(changed["skills"].as_array().unwrap().len(), 1);
    assert_eq!(changed["skills"][0]["name"], "overdue-invoices");

    // Ravi creates it himself through the ordinary save; then the draft is done.
    db.save_agent_profile(&ravi, &w, &agent(&penny, "Penny"))
        .await
        .unwrap();
    assert_eq!(
        db.agent_setup(&ravi, &w, &second.reply).await.unwrap()["created"],
        true
    );
    let third = asked(&db, &ravi, &w, &room, &mine, "Now one for returns.").await;
    assert!(db.agent_setup_context(&third).await.unwrap().is_none());
    assert_eq!(
        hand_back(&db, &third, json!({"part": "name", "text": "Remy"})).await,
        "noted"
    );
    db.chat_finish(&third, "Here is Remy.", "complete")
        .await
        .unwrap();
    let remy = db.agent_setup(&ravi, &w, &third.reply).await.unwrap();
    assert_ne!(
        remy["agent_id"],
        penny.as_str(),
        "a new agent starts afresh"
    );
    assert!(remy["ground_rules"].is_null());

    // A day later an abandoned draft is no longer carried.
    execute(
        &db.db,
        "UPDATE relay_agent_setups SET created_ms=created_ms-90000000",
        vec![],
    )
    .await
    .unwrap();
    let later = asked(&db, &ravi, &w, &room, &mine, "Hello again.").await;
    assert!(db.agent_setup_context(&later).await.unwrap().is_none());
}

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn only_an_assistant_in_its_own_chat_with_its_person_begins_a_draft() {
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
    let room = room_of(&db, &mine).await;
    let name = || json!({"part": "name", "text": "Mole"});

    // An ordinary agent asked in its own direct chat can't.
    let atlas = agent(&uuid::Uuid::new_v4().to_string(), "Atlas");
    db.save_agent_profile(&ravi, &w, &atlas).await.unwrap();
    let direct = db
        .chat_create(
            &ravi,
            &w,
            &NewChat {
                id: uuid::Uuid::new_v4().to_string(),
                name: "Direct".into(),
                kind: "direct".into(),
                description: String::new(),
                users: vec![],
                agents: vec![atlas.id.clone()],
            },
        )
        .await
        .unwrap();
    let job = asked(&db, &ravi, &w, &direct, &atlas.id, "Make me an agent.").await;
    assert_eq!(hand_back(&db, &job, name()).await, "invalid");

    // Nor the assistant in the thread of its drafts.
    let with_priya = db
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
    db.chat_send(&priya, &w, &with_priya, &message("Friday at 3?", None))
        .await
        .unwrap();
    let request = DraftRequest {
        id: uuid::Uuid::new_v4().to_string(),
        chat: with_priya,
        thread: None,
        instruction: "say yes".into(),
        message: None,
    };
    let (_, draft) = db.draft_request(&ravi, &w, &request).await.unwrap();
    let draft = super::super::super::handoff_tests::claim(&db, &draft).await;
    assert_eq!(hand_back(&db, &draft, name()).await, "invalid");
    assert_eq!(
        one(&db, "SELECT count(*) FROM relay_agent_setups", vec![]).await,
        0
    );

    // In its own chat it can, and only its person sees the card.
    let job = asked(&db, &ravi, &w, &room, &mine, "Make me an agent.").await;
    assert_eq!(hand_back(&db, &job, name()).await, "noted");
    db.chat_finish(&job, "Here is Mole.", "complete")
        .await
        .unwrap();
    assert!(card(&db, &ravi, &w, &room, &job.reply).await);
    assert!(db.agent_setup(&priya, &w, &job.reply).await.is_err());
}

//! The preview people read is exactly what a request gets.
use super::skills::NewSkill;
use super::skills::files::SkillFile;
use super::tests::*;
use super::*;
use crate::identity::agents::{config::Settings, system_prompt};
use crate::identity::chat::execution::prepare;

async fn direct_instructions(db: &PgStore, owner: &User, w: &str, agent: &str) -> String {
    let chat = db
        .chat_create(
            owner,
            w,
            &crate::identity::chat::model::NewChat {
                id: uuid::Uuid::new_v4().to_string(),
                name: "Direct".into(),
                kind: "direct".into(),
                description: String::new(),
                users: vec![],
                agents: vec![agent.into()],
            },
        )
        .await
        .unwrap();
    let job = db
        .chat_send(owner, w, &chat, &message("Where is my order?", Some(agent)))
        .await
        .unwrap()
        .unwrap();
    let job = super::handoff_tests::claim(db, &job).await;
    prepare(db, &job, 180).await.unwrap().instructions
}

async fn preview(db: &PgStore, w: &str, agent: &str) -> String {
    let settings: Settings = db.agent_settings(w, agent).await.unwrap().unwrap();
    let features = db.prompt_features(w, agent).await.unwrap();
    system_prompt::prompt("Pool agent", "Bounded fixture", &settings, &features)
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn the_preview_is_exactly_what_a_direct_request_gets() {
    let (db, owner, w, _, agent) = super::automation_tests::setup().await;
    let input = crate::identity::agents::config::SaveAgent {
        id: agent.clone(),
        name: "Pool agent".into(),
        role: "Bounded fixture".into(),
        model: "fixture".into(),
        audience: "selected".into(),
        people: None,
        managers: None,
        revision: 1,
        settings: Settings {
            provider: "openai".into(),
            instructions: "Answer order questions briefly.".into(),
            ..Default::default()
        },
    };
    db.save_agent_profile(&owner, &w, &input).await.unwrap();
    let request = direct_instructions(&db, &owner, &w, &agent).await;
    assert_eq!(preview(&db, &w, &agent).await, request);
    assert!(request.contains("Answer order questions briefly."));
    assert!(request.contains(system_prompt::WORKING_VOICE));

    // A skill adds its section and listing to both.
    let files = vec![SkillFile {
        path: "SKILL.md".into(),
        text: "---\nname: refund-check\ndescription: Use when a customer asks for a refund\n---\n1. Check the order.\n".into(),
    }];
    let skill = db
        .create_skill(
            &owner,
            &w,
            &NewSkill {
                id: uuid::Uuid::new_v4().to_string(),
                shared: false,
                files,
            },
        )
        .await
        .unwrap();
    db.assign_skill(&owner, &w, &agent, skill["id"].as_str().unwrap(), true)
        .await
        .unwrap();
    let request = direct_instructions(&db, &owner, &w, &agent).await;
    assert!(request.contains("\n- refund-check: Use when a customer asks for a refund"));
    assert_eq!(preview(&db, &w, &agent).await, request);
}

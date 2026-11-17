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

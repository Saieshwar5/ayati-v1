//! An emoji-only message gets each asked agent's reaction instead of a model
//! run (D246). Reactions as context feedback: reaction_feedback_tests.rs.
use super::super::{ModelCall, model_budgets::BudgetInput, tests::*, *};
use crate::identity::{
    agents::config::{SaveAgent, Settings},
    chat::model::{NewChat, NewMessage},
};
use sea_orm_migration::MigratorTrait;
use serde_json::json;

async fn count(db: &PgStore, sql: &str, id: &str) -> i64 {
    query(&db.db, sql, vec![id.into()])
        .await
        .unwrap()
        .unwrap()
        .try_get("", "n")
        .unwrap()
}

async fn jobs(db: &PgStore, w: &str) -> i64 {
    count(
        db,
        "SELECT count(*) AS n FROM relay_chat_jobs WHERE workspace=$1",
        w,
    )
    .await
}

/// The message as `viewer` reads it in Chat.
async fn shown(db: &PgStore, viewer: &User, w: &str, chat: &str, id: &str) -> Value {
    db.chat_messages(viewer, w, chat).await.unwrap()["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == id)
        .cloned()
        .unwrap()
}

/// The agents' reactions on a message, as (agent, emoji) by agent.
async fn agent_reactions(db: &PgStore, message: &str) -> Vec<(String, String)> {
    rows(
        &db.db,
        "SELECT agent,emoji FROM relay_agent_reactions WHERE message=$1 ORDER BY agent",
        vec![message.into()],
    )
    .await
    .unwrap()
    .into_iter()
    .map(|r| {
        (
            r.try_get("", "agent").unwrap(),
            r.try_get("", "emoji").unwrap(),
        )
    })
    .collect()
}

async fn named_agent(db: &PgStore, owner: &User, w: &str, name: &str) -> String {
    let input = SaveAgent {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.into(),
        role: "Stock checks".into(),
        model: "fixture".into(),
        audience: "everyone".into(),
        people: None,
        managers: None,
        revision: 0,
        settings: Settings {
            provider: "openai".into(),
            ..Default::default()
        },
    };
    db.save_agent_profile(owner, w, &input).await.unwrap();
    input.id
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn an_emoji_only_direct_message_gets_the_agents_reaction_instead_of_a_run() {
    let db = fixture().await;
    let owner = user(&db, "nod-owner").await;
    let w = workspace(&db, &owner).await;
    let (_, a) = super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    let dm = db
        .chat_create(
            &owner,
            &w,
            &NewChat {
                id: uuid::Uuid::new_v4().to_string(),
                name: "Direct".into(),
                kind: "direct".into(),
                description: String::new(),
                users: vec![],
                agents: vec![a.clone()],
            },
        )
        .await
        .unwrap();
    let thanks = message(" 👍 ", Some(&a));
    assert!(
        db.chat_send_all(&owner, &w, &dm, &thanks)
            .await
            .unwrap()
            .is_empty()
    );
    // No model call, budget or executor slot: there is no job, task or reply.
    assert_eq!(jobs(&db, &w).await, 0);
    for sql in [
        "SELECT count(*) AS n FROM relay_executor_tasks WHERE workspace=$1",
        "SELECT count(*) AS n FROM relay_chat_messages WHERE workspace=$1 AND kind='agent'",
    ] {
        assert_eq!(count(&db, sql, &w).await, 0, "{sql}");
    }
    // The message is stored as sent, with the agent's reaction and its ticks.
    let sent = shown(&db, &owner, &w, &dm, &thanks.id).await;
    assert_eq!(
        (sent["content"].as_str(), sent["state"].as_str()),
        (Some(" 👍 "), Some("sent"))
    );
    assert_eq!(
        sent["agent_reactions"],
        json!([{"agent":a,"emoji":"👍","name":"Pool agent"}])
    );
    assert_eq!(sent["reactions"], json!([]), "people's reactions are apart");
    assert_eq!(
        count(
            &db,
            "SELECT count(*) AS n FROM relay_agent_receipts WHERE message=$1 AND seen_at IS NOT NULL",
            &thanks.id
        )
        .await,
        1
    );
    assert_eq!(
        (sent["delivered"].as_i64(), sent["seen"].as_i64()),
        (Some(1), Some(1))
    );
    // An outbox retry of the same message changes nothing.
    assert!(
        db.chat_send_all(&owner, &w, &dm, &thanks)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(agent_reactions(&db, &thanks.id).await.len(), 1);
    // One of the twelve reactions is mirrored; any other emoji gets 👍.
    for (content, emoji) in [("🎉🎉", "🎉"), ("❤", "❤️"), ("🚚", "👍")] {
        let nod = message(content, Some(&a));
        db.chat_send_all(&owner, &w, &dm, &nod).await.unwrap();
        assert_eq!(
            agent_reactions(&db, &nod.id).await,
            [(a.clone(), emoji.into())]
        );
    }
    assert_eq!(jobs(&db, &w).await, 0);
    // Deleting the message removes the agent's reaction with it.
    db.delete_message(&owner, &w, &dm, &thanks.id)
        .await
        .unwrap();
    assert!(agent_reactions(&db, &thanks.id).await.is_empty());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn an_emoji_only_message_to_two_agents_or_a_quote_gets_their_reactions() {
    let db = fixture().await;
    let owner = user(&db, "nod-two-owner").await;
    let w = workspace(&db, &owner).await;
    let atlas = named_agent(&db, &owner, &w, "Atlas").await;
    let nova = named_agent(&db, &owner, &w, "Nova").await;
    let c = db
        .chat_create(
            &owner,
            &w,
            &NewChat {
                id: uuid::Uuid::new_v4().to_string(),
                name: "Stock".into(),
                kind: "channel".into(),
                description: String::new(),
                users: vec![],
                agents: vec![atlas.clone(), nova.clone()],
            },
        )
        .await
        .unwrap();
    let both = NewMessage {
        agents: vec![atlas.clone(), nova.clone()],
        ..message(
            &format!("[@Atlas](mention:agent:{atlas}) [@Nova](mention:agent:{nova}) 🙏"),
            None,
        )
    };
    let cursor = db.sync_page(&owner, &w, None).await.unwrap()["cursor"]
        .as_i64()
        .unwrap();
    assert!(
        db.chat_send_all(&owner, &w, &c, &both)
            .await
            .unwrap()
            .is_empty()
    );
    let mut expected = vec![
        (atlas.clone(), "🙏".to_string()),
        (nova.clone(), "🙏".into()),
    ];
    expected.sort();
    assert_eq!(agent_reactions(&db, &both.id).await, expected);
    assert_eq!(
        count(
            &db,
            "SELECT count(*) AS n FROM relay_agent_receipts WHERE message=$1 AND seen_at IS NOT NULL",
            &both.id
        )
        .await,
        2,
        "each agent read it"
    );
    // Open screens get the reactions as a change to that message.
    let page = db.sync_page(&owner, &w, Some(cursor)).await.unwrap();
    let changed = page["events"]
        .as_array()
        .unwrap()
        .iter()
        .rev()
        .find(|e| e["kind"] == "message" && e["message"]["id"] == both.id)
        .unwrap()
        .clone();
    assert_eq!(
        changed["message"]["agent_reactions"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    // The exact replay asks the same two agents and starts nothing.
    assert!(
        db.chat_send_all(&owner, &w, &c, &both)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(jobs(&db, &w).await, 0);

    // Quoting an agent's reply with only emoji gets that agent's reaction.
    let asked = db
        .chat_send(
            &owner,
            &w,
            &c,
            &message(
                &format!("[@Atlas](mention:agent:{atlas}) stock?"),
                Some(&atlas),
            ),
        )
        .await
        .unwrap()
        .unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    db.chat_finish(&job, "12 boxes.", "complete").await.unwrap();
    let quote = NewMessage {
        reply_to: Some(asked.reply.clone()),
        ..message("👏", Some(&atlas))
    };
    assert!(
        db.chat_send_all(&owner, &w, &c, &quote)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        agent_reactions(&db, &quote.id).await,
        [(atlas.clone(), "👏".into())]
    );
    assert_eq!(jobs(&db, &w).await, 1);

    // Nothing runs, so an agent past its monthly budget still reacts; words
    // still need the budget.
    db.save_model_budget(
        &owner,
        &w,
        &BudgetInput {
            agent: nova.clone(),
            monthly_usd: Some(1.0),
            warn_percent: 80,
        },
    )
    .await
    .unwrap();
    db.record_model_usage(&ModelCall {
        workspace: &w,
        assignment: "fixture-assignment",
        agent: &nova,
        job: None,
        provider: "fireworks",
        model: "llama-70b",
        provider_model: "llama",
        attempts: 1,
        input_tokens: 1,
        output_tokens: 1,
        person: None,
        charge_micro: 1_000_000,
        cost_micro: 0,
        latency_ms: None,
    })
    .await
    .unwrap();
    let to_nova = |text: &str| NewMessage {
        agents: vec![nova.clone()],
        ..message(&format!("[@Nova](mention:agent:{nova}) {text}"), None)
    };
    let nod = to_nova("💯");
    assert!(
        db.chat_send_all(&owner, &w, &c, &nod)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        agent_reactions(&db, &nod.id).await,
        [(nova.clone(), "💯".into())]
    );
    assert!(
        db.chat_send_all(&owner, &w, &c, &to_nova("stock?"))
            .await
            .is_err()
    );
    // A paused agent refuses even emoji, as it refuses any message, and
    // nothing is saved.
    db.agent_pause_set(&owner, &w, &atlas, true).await.unwrap();
    let paused = NewMessage {
        agents: vec![atlas.clone()],
        ..message(&format!("[@Atlas](mention:agent:{atlas}) 👍"), None)
    };
    let error = db.chat_send_all(&owner, &w, &c, &paused).await.unwrap_err();
    assert!(error.to_string().contains("paused"), "{error}");
    assert_eq!(
        count(
            &db,
            "SELECT count(*) AS n FROM relay_chat_messages WHERE id=$1",
            &paused.id
        )
        .await,
        0
    );
    assert_eq!(jobs(&db, &w).await, 1);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn emoji_with_words_runs_normally_and_emoji_alone_asks_no_one_by_itself() {
    let db = fixture().await;
    let owner = user(&db, "nod-words").await;
    let w = workspace(&db, &owner).await;
    let (c, a) = super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    let ask = message("👍 thanks, now check the pallets", Some(&a));
    let job = db.chat_send(&owner, &w, &c, &ask).await.unwrap();
    assert!(job.is_some(), "words make it a request");
    assert!(agent_reactions(&db, &ask.id).await.is_empty());
    // Emoji that ask no agent are an ordinary message: no reaction, no job.
    let plain = message("🎉", None);
    assert!(
        db.chat_send(&owner, &w, &c, &plain)
            .await
            .unwrap()
            .is_none()
    );
    assert!(agent_reactions(&db, &plain.id).await.is_empty());
    assert_eq!(jobs(&db, &w).await, 1);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn schema_101_adds_agent_reactions_and_keeps_peoples() {
    let url = std::env::var("RELAY_TEST_DATABASE_URL").unwrap();
    let admin = PgStore::connect(&url).await.unwrap();
    let schema = format!("relay_agent_reactions_{}", uuid::Uuid::new_v4().simple());
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
    super::super::migration::Migrator::up(&db.db, Some(100))
        .await
        .unwrap();
    assert!(db.verify_schema().await.is_err(), "schema 100 is refused");
    let owner = user(&db, "agent-reactions-upgrade").await;
    let w = workspace(&db, &owner).await;
    let (c, a) = super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    let note = message("Before the upgrade", None);
    db.chat_send(&owner, &w, &c, &note).await.unwrap();
    db.react(&owner, &w, &c, &note.id, "👍", true)
        .await
        .unwrap();

    db.migrate().await.unwrap();
    db.verify_schema().await.unwrap();
    let kept = shown(&db, &owner, &w, &c, &note.id).await;
    assert_eq!(
        kept["reactions"],
        json!([{"emoji":"👍","count":1,"mine":true,"users":[owner.id.clone()]}])
    );
    assert_eq!(kept["agent_reactions"], json!([]));
    let nod = message("👍", Some(&a));
    assert!(db.chat_send(&owner, &w, &c, &nod).await.unwrap().is_none());
    assert_eq!(agent_reactions(&db, &nod.id).await, [(a, "👍".into())]);
    assert_eq!(jobs(&db, &w).await, 0);
}

//! Agents learn from their work (schema 121): signals come only from shared
//! conversations, lessons are checked before anyone sees them, and only an
//! approved lesson becomes a note.
use super::super::agent_knowledge::KnowledgeInput;
use super::super::tests::*;
use super::*;
use crate::identity::agent_learning::learn;
use crate::identity::chat::model::Job;
use crate::identity::library::models::Settings;
use axum::{Json, Router, routing::post};

/// A fake model that proposes three good lessons and three it must not keep.
async fn provider() -> std::path::PathBuf {
    let router = Router::new().route(
        "/v1/chat/completions",
        post(|Json(input): Json<Value>| async move {
            let asked = input["messages"][1]["content"].as_str().unwrap_or("").to_owned();
            assert!(!asked.contains("48000"), "a private chat was read");
            let note = asked
                .lines()
                .filter_map(|line| serde_json::from_str::<Value>(line).ok())
                .find(|line| line["title"] == "Delivery questions")
                .map(|note| note["id"].clone())
                .unwrap_or(Value::Null);
            let lesson = |title: &str, what: &str, evidence: &str, kind: &str, replaces: Value| json!({
                "title": title, "when": "A customer asks about delivery", "do": what,
                "why": "People asked again after the first answer", "evidence": [evidence], "kind": kind, "replaces": replaces
            });
            let lessons = json!([
                lesson("Delivery date first", "Give the expected delivery date before the courier's name", "s1", "new", Value::Null),
                lesson("Delivery questions", "Always include the expected delivery date", "s1", "update", note),
                lesson("Confirm the warehouse", "Say which warehouse the order ships from", "s2", "new", Value::Null),
                lesson("Ask Meera", "Ask Meera before promising a date", "s1", "new", Value::Null),
                lesson("Refund limit", "Refunds over ₹4500 need a manager", "s1", "new", Value::Null),
                lesson("Made up", "Something no signal shows", "s99", "new", Value::Null),
            ]);
            Json(json!({"choices":[{"message":{"content":json!({"lessons": lessons}).to_string()}}],
                "usage":{"prompt_tokens":700,"completion_tokens":200}}))
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    let dir = std::env::temp_dir().join(format!("learning-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("key"), "fixture-key").unwrap();
    std::fs::write(
        dir.join("providers.json"),
        json!([{"provider":"openai","base_url":format!("http://{address}/v1"),"key_file":dir.join("key")}]).to_string(),
    )
    .unwrap();
    dir.join("providers.json")
}

/// A person asks the agent and it answers, as an executor would finish it.
async fn answered(
    db: &PgStore,
    person: &User,
    w: &str,
    c: &str,
    a: &str,
    ask: &str,
    answer: &str,
) -> Job {
    db.chat_send(person, w, c, &message(ask, Some(a)))
        .await
        .unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    db.chat_finish(&job, answer, "complete").await.unwrap();
    job
}

fn now_ms() -> i64 {
    now() * 1000 + 60_000
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn lessons_come_from_shared_conversations_and_reach_the_agent_only_when_approved() {
    let db = fixture().await;
    let owner = user(&db, "learning-owner").await;
    let other = user(&db, "learning-member").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &other, "Member").await;
    execute(
        &db.db,
        "UPDATE relay_users SET display_name='Meera Nair' WHERE id=$1",
        vec![other.id.clone().into()],
    )
    .await
    .unwrap();
    let (private, a) = super::super::executor_slot_tests::agent(&db, &owner, &w).await;
    db.chat_grant(&owner, &w, &other.id, &a, true)
        .await
        .unwrap();
    let shared = chat(&db, &owner, &w, vec![other.id.clone()], vec![a.clone()]).await;
    db.save_agent_knowledge(
        &owner,
        &w,
        &a,
        &KnowledgeInput {
            id: uuid::Uuid::new_v4().to_string(),
            chat: None,
            title: "Delivery questions".into(),
            content: "Name the courier.".into(),
            source_message: None,
            revision: 0,
            forgotten: false,
        },
    )
    .await
    .unwrap();

    // Work in a shared conversation: a correction and a reaction.
    let first = answered(
        &db,
        &owner,
        &w,
        &shared,
        &a,
        "When does order 7 arrive?",
        "It goes by BlueDart.",
    )
    .await;
    // The answer is in a thread under the question (schema 123), and so is the correction.
    let thread: Option<String> = query(
        &db.db,
        "SELECT thread FROM relay_chat_messages WHERE workspace=$1 AND id=$2",
        vec![w.clone().into(), first.reply.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "thread")
    .unwrap();
    assert!(thread.is_some());
    db.chat_send(
        &other,
        &w,
        &shared,
        &crate::identity::chat::model::NewMessage {
            thread,
            ..message("That doesn't help, we need the delivery date", None)
        },
    )
    .await
    .unwrap();
    db.react(&other, &w, &shared, &first.reply, "🤔", true)
        .await
        .unwrap();
    answered(
        &db,
        &owner,
        &w,
        &shared,
        &a,
        "Which warehouse ships it?",
        "It ships soon.",
    )
    .await;
    db.chat_send(
        &owner,
        &w,
        &shared,
        &message("Please always say the warehouse", None),
    )
    .await
    .unwrap();
    // A private chat is never read.
    answered(
        &db,
        &owner,
        &w,
        &private,
        &a,
        "My salary is 48000, check my tax",
        "Done.",
    )
    .await;
    db.chat_send(
        &owner,
        &w,
        &private,
        &message("No, my salary is 48000", None),
    )
    .await
    .unwrap();

    let at = now_ms();
    assert_eq!(
        db.learning_due(at, 10).await.unwrap(),
        vec![(w.clone(), a.clone())]
    );
    let file = provider().await;
    let settings = Settings {
        file: &file,
        provider: "openai",
        model: "fixture-learning",
    };
    assert_eq!(
        learn(&db, &settings, &w, &a, at).await.unwrap(),
        3,
        "three lessons pass the checks"
    );
    assert!(
        db.learning_due(at + 60_000, 10).await.unwrap().is_empty(),
        "once a day"
    );
    let usage = query(&db.db, "SELECT count(*) AS n FROM relay_model_usage WHERE workspace=$1 AND assignment='learning' AND agent=$2",
        vec![w.clone().into(), a.clone().into()]).await.unwrap().unwrap();
    assert_eq!(usage.try_get::<i64>("", "n").unwrap(), 1);

    // Only the agent's maintainers review lessons, with the evidence they may open.
    assert!(db.agent_learning(&other, &w, &a).await.is_err());
    let review = db.agent_learning(&owner, &w, &a).await.unwrap();
    let proposals = review["proposals"].as_array().unwrap();
    assert_eq!(proposals.len(), 3);
    let text = review.to_string();
    assert!(!text.contains("Meera") && !text.contains("4500") && !text.contains("Made up"));
    let by_title = |title: &str| {
        proposals
            .iter()
            .find(|p| p["title"] == title)
            .unwrap()
            .clone()
    };
    let date = by_title("Delivery date first");
    assert_eq!(
        date["conversation"]["id"],
        shared.as_str(),
        "it came from one conversation"
    );
    assert!(date["evidence"].to_string().contains("BlueDart"), "{date}");
    let update = by_title("Delivery questions");
    assert_eq!(update["kind"], "update");
    assert_eq!(update["replaces"]["content"], "Name the courier.");

    // Nothing reaches the agent before approval.
    let notes = db.agent_knowledge(&owner, &w, &a).await.unwrap();
    assert_eq!(notes.len(), 1);

    // Approved for its conversation only, edited, or replacing the note it updates.
    let decide = |approve: bool, content: Option<&str>| Decision {
        approve,
        title: None,
        content: content.map(str::to_owned),
        everywhere: false,
    };
    let id = |p: &Value| p["id"].as_str().unwrap().to_owned();
    let approved = db
        .decide_lesson(&owner, &w, &a, &id(&date), &decide(true, None))
        .await
        .unwrap();
    assert_eq!(approved["state"], "approved");
    db.decide_lesson(
        &owner,
        &w,
        &a,
        &id(&update),
        &decide(true, Some("Give the date first, then the courier.")),
    )
    .await
    .unwrap();
    let warehouse = by_title("Confirm the warehouse");
    db.decide_lesson(&owner, &w, &a, &id(&warehouse), &decide(false, None))
        .await
        .unwrap();
    // A second answer changes nothing.
    let again = db
        .decide_lesson(&owner, &w, &a, &id(&warehouse), &decide(true, None))
        .await
        .unwrap();
    assert_eq!(again["state"], "rejected");

    let notes = db.agent_knowledge(&owner, &w, &a).await.unwrap();
    assert_eq!(
        notes.len(),
        2,
        "one new note, one updated, none for the rejected lesson"
    );
    let learned = notes
        .iter()
        .find(|n| n["title"] == "Delivery date first")
        .unwrap();
    assert_eq!(learned["chat"], shared.as_str());
    assert_eq!(learned["learned_from"], id(&date).as_str());
    let updated = notes
        .iter()
        .find(|n| n["title"] == "Delivery questions")
        .unwrap();
    assert_eq!(updated["content"], "Give the date first, then the courier.");
    assert_eq!(updated["revision"], 1);

    // Learning can be switched off by a maintainer.
    let off = db
        .set_agent_learning(&owner, &w, &a, false, review["revision"].as_i64().unwrap())
        .await
        .unwrap();
    assert_eq!(off["enabled"], false);
    assert!(
        db.set_agent_learning(&owner, &w, &a, true, 0)
            .await
            .is_err(),
        "a stale switch is refused"
    );
    assert!(
        db.learning_due(at + 2 * EVERY_MS, 10)
            .await
            .unwrap()
            .is_empty()
    );
}

#[test]
fn lessons_never_name_a_person_or_carry_numbers_or_amounts() {
    let names = vec!["Meera Nair".to_owned(), "Al".to_owned()];
    for text in [
        "Ask Meera before promising a date",
        "Copy meera nair on refunds",
        "Refunds over ₹4500 need a manager",
        "Refunds over Rs. 300 need a manager",
        "Email support@example.com first",
        "Order 784512 is late",
    ] {
        assert!(personal(text, &names), "{text}");
    }
    for text in [
        "Give the delivery date before the courier's name",
        "Always say which warehouse ships the order",
        "Answer within 2 days",
    ] {
        assert!(!personal(text, &names), "{text}");
    }
}

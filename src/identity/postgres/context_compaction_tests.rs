//! Automatic working summaries (schema 120): written in the background for a
//! long conversation, from that room's messages only, and saved as the room's
//! next summary version unless the room changed meanwhile.
use super::{tests::*, *};
use crate::identity::chat::{execution::prepare, model::Job};
use crate::identity::context_compaction::round;
use crate::identity::library::models::Settings;
use axum::{Json, Router, routing::post};
use serde_json::json;
use std::sync::{Arc, Mutex};

/// A fake model provider that records what it was asked and answers with a
/// summary citing the first and last message it was shown, plus one id it
/// was never shown.
async fn provider() -> (std::path::PathBuf, Arc<Mutex<Vec<String>>>) {
    let asked = Arc::new(Mutex::new(Vec::new()));
    let seen = asked.clone();
    let router = Router::new().route(
        "/v1/chat/completions",
        post(move |Json(input): Json<Value>| {
            let seen = seen.clone();
            async move {
                let request = input["messages"][1]["content"]
                    .as_str()
                    .unwrap_or("")
                    .to_owned();
                let ids: Vec<String> = request
                    .lines()
                    .filter_map(|line| serde_json::from_str::<Value>(line).ok())
                    .filter_map(|m| m["id"].as_str().map(str::to_owned))
                    .collect();
                seen.lock().unwrap().push(request);
                let answer = json!({
                    "objective": {"text": "Plan the Pune warehouse move", "sources": [ids.first()]},
                    "decisions": [
                        {"text": "Move happens in March", "sources": [ids.last()]},
                        {"text": "Invented by the model", "sources": ["never-shown"]}
                    ],
                    "questions": [], "corrections": []
                });
                Json(
                    json!({"choices":[{"message":{"content":format!("```json\n{answer}\n```")}}],
                    "usage":{"prompt_tokens":900,"completion_tokens":60}}),
                )
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    let dir = std::env::temp_dir().join(format!("context-summary-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("key"), "fixture-key").unwrap();
    std::fs::write(
        dir.join("providers.json"),
        json!([{"provider":"openai","base_url":format!("http://{address}/v1"),"key_file":dir.join("key")}]).to_string(),
    )
    .unwrap();
    (dir.join("providers.json"), asked)
}

fn settings(file: &std::path::Path) -> Settings<'_> {
    Settings {
        file,
        provider: "openai",
        model: "fixture-summary",
    }
}

/// A request claimed by an executor, with its context prepared and saved.
async fn ask(db: &PgStore, person: &User, w: &str, c: &str, a: &str, text: &str) -> (Job, String) {
    db.chat_send(person, w, c, &message(text, Some(a)))
        .await
        .unwrap();
    let job = db.claim_job().await.unwrap().unwrap();
    let prepared = prepare(db, &job, 180).await.unwrap();
    (job, prepared.request)
}

/// One request and reply, run as an executor would: claimed, prepared, finished.
async fn exchange(db: &PgStore, person: &User, w: &str, c: &str, a: &str, text: &str) -> Job {
    let (job, _) = ask(db, person, w, c, a, text).await;
    db.chat_finish(&job, &format!("Noted: {text}"), "complete")
        .await
        .unwrap();
    job
}

fn long(n: usize) -> String {
    format!(
        "Step {n} of the Pune warehouse move: {}",
        "racks, labels and the loading dock schedule. ".repeat(14)
    )
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_long_conversation_gets_an_automatic_summary_that_stays_in_its_room() {
    let db = fixture().await;
    let owner = user(&db, "summary-owner").await;
    let other = user(&db, "summary-other").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &other, "Member").await;
    let (c, a) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    db.chat_grant(&owner, &w, &other.id, &a, true)
        .await
        .unwrap();
    let other_chat = chat(&db, &other, &w, vec![], vec![a.clone()]).await;
    let (file, asked) = provider().await;
    let settings = settings(&file);
    let mut after = (0, String::new());

    // A short conversation needs no summary and costs no model call.
    let first = exchange(&db, &owner, &w, &c, &a, &long(0)).await;
    exchange(
        &db,
        &other,
        &w,
        &other_chat,
        &a,
        "My salary is 48000, keep it private",
    )
    .await;
    assert!(round(&db, &settings, &mut after).await);
    assert!(asked.lock().unwrap().is_empty());
    assert!(
        db.compaction_input(&w, &first.reply)
            .await
            .unwrap()
            .is_none()
    );

    // Once it outgrows a prompt's recent window, the room gets a summary.
    let mut last = first;
    for n in 1..12 {
        last = exchange(&db, &owner, &w, &c, &a, &long(n)).await;
    }
    assert!(round(&db, &settings, &mut after).await);
    let requests = asked.lock().unwrap().clone();
    assert_eq!(requests.len(), 1, "one model call for the room");
    assert!(requests[0].contains("Step 11 of the Pune warehouse move"));
    assert!(
        !requests[0].contains("48000"),
        "another person's room is never read"
    );

    // The next request's prompt carries the summary.
    let (next, request) = ask(&db, &owner, &w, &c, &a, "What is left?").await;
    assert!(request.contains("Move happens in March"), "{request}");
    let working = db.working_context(&next).await.unwrap();
    assert_eq!(working["revision"], 1);
    assert_eq!(working["made_by"], "automatic");
    assert_eq!(working["task"], last.reply.as_str());
    let brief = working["brief"].to_string();
    assert!(
        brief.contains("Plan the Pune warehouse move") && brief.contains("Move happens in March")
    );
    assert!(
        !brief.contains("never-shown") && !brief.contains("Invented"),
        "uncited entries are dropped"
    );
    let usage = query(&db.db, "SELECT count(*) AS n FROM relay_model_usage WHERE workspace=$1 AND assignment='context' AND agent=$2 AND job=$3",
        vec![w.clone().into(), a.clone().into(), last.reply.clone().into()]).await.unwrap().unwrap();
    assert_eq!(usage.try_get::<i64>("", "n").unwrap(), 1);

    // The other person's private room has no summary, and nothing new means no call.
    let theirs = query(
        &db.db,
        "SELECT count(*) AS n FROM relay_working_context WHERE workspace=$1 AND chat=$2",
        vec![w.clone().into(), other_chat.clone().into()],
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(theirs.try_get::<i64>("", "n").unwrap(), 0);
    round(&db, &settings, &mut after).await;
    assert_eq!(asked.lock().unwrap().len(), 1);

    // The agent's owner sees which summaries were written automatically.
    let preview = db.agent_context_preview(&owner, &w, &a, &c).await.unwrap();
    assert_eq!(preview["working"]["made_by"], "automatic");
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_summary_written_while_the_room_changed_is_dropped() {
    let db = fixture().await;
    let owner = user(&db, "summary-change").await;
    let w = workspace(&db, &owner).await;
    let (c, a) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    let mut last = exchange(&db, &owner, &w, &c, &a, &long(0)).await;
    for n in 1..12 {
        last = exchange(&db, &owner, &w, &c, &a, &long(n)).await;
    }
    let input = db
        .compaction_input(&w, &last.reply)
        .await
        .unwrap()
        .expect("long enough");
    let ids: Vec<&str> = input
        .messages
        .iter()
        .filter_map(|m| m["id"].as_str())
        .collect();
    let answer = json!({"objective":{"text":"Plan the move","sources":[ids[0]]},"decisions":[],"questions":[],"corrections":[]});

    // Approved knowledge changes what the agent may see: the summary is not saved.
    db.save_agent_knowledge(
        &owner,
        &w,
        &a,
        &super::agent_knowledge::KnowledgeInput {
            id: uuid::Uuid::new_v4().to_string(),
            chat: None,
            title: "Dock hours".into(),
            content: "The loading dock opens at 7.".into(),
            source_message: None,
            revision: 0,
            forgotten: false,
        },
    )
    .await
    .unwrap();
    assert!(
        !db.save_compaction(&input, &answer, "fixture-summary")
            .await
            .unwrap()
    );

    // The change also took the agent's earlier replies out of its history, as it
    // does for every later prompt. Once the room grows again, a new reading saves;
    // an answer citing nothing it read does not.
    for n in 12..24 {
        last = exchange(&db, &owner, &w, &c, &a, &long(n)).await;
    }
    let fresh = db
        .compaction_input(&w, &last.reply)
        .await
        .unwrap()
        .expect("still long");
    let invented = json!({"objective":{"text":"Made up","sources":["never-shown"]}});
    assert!(
        !db.save_compaction(&fresh, &invented, "fixture-summary")
            .await
            .unwrap()
    );
    let ids: Vec<&str> = fresh
        .messages
        .iter()
        .filter_map(|m| m["id"].as_str())
        .collect();
    let answer = json!({"objective":{"text":"Plan the move","sources":[ids[0]]},"decisions":[],"questions":[],"corrections":[]});
    assert!(
        db.save_compaction(&fresh, &answer, "fixture-summary")
            .await
            .unwrap()
    );
    // A second save of the same reading loses to the first.
    assert!(
        !db.save_compaction(&fresh, &answer, "fixture-summary")
            .await
            .unwrap()
    );
    // Saved summaries stay immutable outside forgetting.
    assert!(
        execute(
            &db.db,
            "DELETE FROM relay_working_context WHERE workspace=$1",
            vec![w.clone().into()]
        )
        .await
        .is_err()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn a_shared_conversation_gets_one_summary_everyone_there_reads() {
    let db = fixture().await;
    let owner = user(&db, "summary-room-owner").await;
    let other = user(&db, "summary-room-other").await;
    let w = workspace(&db, &owner).await;
    join(&db, &owner, &w, &other, "Member").await;
    let (_, a) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    db.chat_grant(&owner, &w, &other.id, &a, true)
        .await
        .unwrap();
    let room = chat(&db, &owner, &w, vec![other.id.clone()], vec![a.clone()]).await;
    let mut last = exchange(&db, &owner, &w, &room, &a, &long(0)).await;
    for n in 1..12 {
        let person = if n % 2 == 0 { &owner } else { &other };
        last = exchange(&db, person, &w, &room, &a, &long(n)).await;
    }
    let input = db
        .compaction_input(&w, &last.reply)
        .await
        .unwrap()
        .expect("long enough");
    assert!(input.shared, "everyone in the room reads it");
    let (file, asked) = provider().await;
    round(&db, &settings(&file), &mut (0, String::new())).await;
    assert_eq!(asked.lock().unwrap().len(), 1);
    // One brief for the room, read by whoever asks next.
    let owners = query(
        &db.db,
        "SELECT string_agg(DISTINCT requester,',') AS r FROM relay_working_context WHERE workspace=$1 AND chat=$2",
        vec![w.clone().into(), room.clone().into()],
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(owners.try_get::<String>("", "r").unwrap(), "");
    let (_, request) = ask(&db, &other, &w, &room, &a, "Where are we?").await;
    assert!(request.contains("Shared brief") && request.contains("Move happens in March"));
}

/// A catalog listing the fixture agents' model with a 128,000-token window.
fn large_window(dir: &std::path::Path) -> crate::identity::models::catalog::Files {
    let providers = dir.join("providers.json");
    std::fs::write(&providers, r#"[{"provider":"fireworks","base_url":"https://api.fireworks.ai/inference/v1","key_file":"/k"}]"#).unwrap();
    let catalog = dir.join("catalog.json");
    std::fs::write(&catalog, r#"{"models":[{"id":"fixture","name":"Fixture","context":128000,"routes":[{"provider":"fireworks","model":"fixture"}]}]}"#).unwrap();
    crate::identity::models::catalog::Files { catalog, providers }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn history_and_summaries_follow_the_models_context_window() {
    let mut db = fixture().await;
    let owner = user(&db, "summary-window").await;
    let w = workspace(&db, &owner).await;
    let (c, a) = super::executor_slot_tests::agent(&db, &owner, &w).await;
    let mut last = exchange(&db, &owner, &w, &c, &a, &long(0)).await;
    for n in 1..20 {
        last = exchange(&db, &owner, &w, &c, &a, &long(n)).await;
    }
    let unlisted = db.clone();
    let dir = tempfile::tempdir().unwrap();
    db.model_catalog = Some(large_window(dir.path()));
    assert_eq!(db.model_window("fixture"), 128_000);

    // Outside the catalog the summary is due; a 128,000-token model still holds
    // these messages word for word, so it waits.
    assert!(
        unlisted
            .compaction_input(&w, &last.reply)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        db.compaction_input(&w, &last.reply)
            .await
            .unwrap()
            .is_none()
    );

    // The next prompt carries the whole conversation for the large model, and
    // only the latest part for a model the catalog doesn't describe.
    let (next, request) = ask(&db, &owner, &w, &c, &a, "What is left?").await;
    assert!(
        request.contains("Step 1 of the Pune warehouse move"),
        "large window"
    );
    let (_, smaller) = unlisted.request_context(&next).await.unwrap();
    assert!(
        !smaller.contains("Step 1 of the Pune warehouse move"),
        "14 KB window"
    );
    assert!(smaller.contains("Step 19 of the Pune warehouse move"));
    // Goose is told the real window for runs that compact as they go.
    let prepared = prepare(&db, &next, 180).await.unwrap();
    assert_eq!(prepared.context_limit, Some(128_000));
}

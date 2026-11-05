//! Library LB2b: the indexer embeds passages and summarises documents through the
//! providers file; search finds a passage by meaning that shares no word with the
//! question, still only among documents usable for the people asking.
use super::super::{tests::*, *};
use super::folders::{ask, setup};
use crate::identity::control::broker::invoke;
use crate::identity::library::models::{Configured, DIMENSIONS};
use axum::{Json, Router, routing::post};
use serde_json::json;

/// Words of a group share one direction; other words hash to their own.
fn meaning(text: &str) -> Vec<f32> {
    let groups: [&[&str]; 2] = [
        &[
            "return", "returns", "refund", "refunds", "refunded", "back", "send",
        ],
        &["salary", "salaries", "pay", "band"],
    ];
    let mut v = vec![0f32; DIMENSIONS];
    for word in text
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
    {
        match groups.iter().position(|g| g.contains(&word)) {
            Some(g) => v[g] += 3.0,
            None => {
                v[8 + word
                    .bytes()
                    .fold(0usize, |h, b| (h * 31 + b as usize) % 1500)] += 1.0
            }
        }
    }
    let length = v.iter().map(|n| n * n).sum::<f32>().sqrt().max(1e-6);
    v.into_iter().map(|n| n / length).collect()
}
async fn provider() -> (Configured, tokio::task::JoinHandle<()>) {
    let router = Router::new()
        .route("/v1/embeddings", post(|Json(input): Json<Value>| async move {
            let data: Vec<Value> = input["input"].as_array().unwrap().iter().enumerate()
                .map(|(i, t)| json!({"index": i, "embedding": meaning(t.as_str().unwrap())})).collect();
            Json(json!({"data": data, "usage": {"prompt_tokens": 7}}))
        }))
        .route("/v1/chat/completions", post(|Json(input): Json<Value>| async move {
            let asked = input["messages"][1]["content"].as_str().unwrap_or("");
            let title = asked.lines().nth(1).unwrap_or("").trim_start_matches("Title: ");
            Json(json!({"choices":[{"message":{"content":format!("How {title} works, for colleagues.")}}],"usage":{"prompt_tokens":40,"completion_tokens":9}}))
        }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    let dir = std::env::temp_dir().join(format!("library-models-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("key"), "fixture-key").unwrap();
    std::fs::write(
        dir.join("providers.json"),
        json!([{"provider":"openai","base_url":format!("http://{address}/v1"),"key_file":dir.join("key")}]).to_string(),
    )
    .unwrap();
    let configured = Configured {
        file: dir.join("providers.json"),
        embedding: Some(("openai".into(), "fixture-embedding".into())),
        summary: Some(("openai".into(), "fixture-summary".into())),
    };
    (configured, server)
}
async fn index(db: &PgStore, configured: &Configured) {
    for _ in 0..20 {
        if !crate::identity::library::indexer::round(db, configured).await {
            return;
        }
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL with pgvector"]
async fn a_question_finds_the_passage_with_its_meaning_and_documents_carry_summaries() {
    let mut s = setup().await;
    let (configured, _server) = provider().await;
    s.db.library_models = Some(configured.clone());
    index(&s.db, &configured).await;
    let db = &s.db;
    let embedded: i64 = query(
        &db.db,
        "SELECT count(*) AS n FROM relay_library_embeddings WHERE workspace=$1",
        vec![s.w.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "n")
    .unwrap();
    assert_eq!(embedded, 2, "both pages' passages are embedded");
    let theirs = chat(db, &s.member, &s.w, vec![], vec![s.agent.clone()]).await;
    let job = ask(&s, &s.member, &theirs, "Can I send something back?").await;
    // No word in common with "The refund window is 30 days from delivery."
    let found = invoke(
        db,
        &job,
        "search_library",
        json!({"query":"can I send something back?"}),
    )
    .await
    .unwrap();
    assert_eq!(found["by_meaning"], true);
    assert_eq!(found["results"][0]["title"], "Returns policy");
    assert_eq!(
        found["results"][0]["summary"],
        "How Returns policy works, for colleagues."
    );
    assert!(
        found["results"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["title"] != "Salaries"),
        "meaning search keeps the audience rules"
    );
    let listed = invoke(db, &job, "list_folder", json!({})).await.unwrap();
    assert_eq!(listed["folders"][0]["title"], "Policies");
    db.chat_finish(&job, "Within 30 days.", "complete")
        .await
        .unwrap();
    // Without the model, the same question finds nothing by words alone.
    let mut words_only = setup().await;
    words_only.db.library_models = None;
    let other = chat(
        &words_only.db,
        &words_only.member,
        &words_only.w,
        vec![],
        vec![words_only.agent.clone()],
    )
    .await;
    let plain = ask(
        &words_only,
        &words_only.member,
        &other,
        "Can I send something back?",
    )
    .await;
    let found = invoke(
        &words_only.db,
        &plain,
        "search_library",
        json!({"query":"can I send something back?"}),
    )
    .await
    .unwrap();
    assert_eq!(found["by_meaning"], false);
    assert!(found["results"].as_array().unwrap().is_empty());
    // Usage is recorded for the workspace, and an edit is embedded again.
    let used: i64 = query(
        &db.db,
        "SELECT count(*) AS n FROM relay_model_usage WHERE workspace=$1 AND assignment='library'",
        vec![s.w.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "n")
    .unwrap();
    assert!(used >= 3, "one embedding batch and two summaries");
    let current = db.library_get(&s.owner, &s.w, &s.returns).await.unwrap();
    db.library_save(&s.owner, &s.w, &crate::identity::library::model::SavePage {
        id: s.returns.clone(),
        request: uuid::Uuid::new_v4().to_string(),
        revision: current["revision"].as_i64().unwrap(),
        title: "Returns policy".into(),
        document: json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Refunds within 45 days."}]}]}),
        folder: None,
    }).await.unwrap();
    let stale: i64 = query(
        &db.db,
        "SELECT count(*) AS n FROM relay_library_embeddings WHERE workspace=$1 AND item=$2",
        vec![s.w.clone().into(), s.returns.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "n")
    .unwrap();
    assert_eq!(stale, 0, "re-cutting passages drops their old embeddings");
    index(db, &configured).await;
    let summary: i64 = query(
        &db.db,
        "SELECT revision FROM relay_library_summaries WHERE workspace=$1 AND item=$2",
        vec![s.w.clone().into(), s.returns.clone().into()],
    )
    .await
    .unwrap()
    .unwrap()
    .try_get("", "revision")
    .unwrap();
    assert_eq!(summary, current["revision"].as_i64().unwrap() + 1);
}

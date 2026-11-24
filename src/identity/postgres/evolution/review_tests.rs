//! Schema 143 end to end with a fake model provider: outcomes read by the
//! classifier hold the notes a correction contradicts, requests are sorted into
//! job types, and a three-day review proposes a skill from routine work, which an
//! owner approves and which then says it was created by the agent. Drafts that
//! name a person or do worse in rehearsal are dropped; a rejected job waits.
use super::super::tests::*;
use super::tests::{answered, field, note, reply_to, team};
use super::*;
use crate::identity::evolution::{Models, round};
use axum::{Json, Router, extract::State, routing::post};
use serde_json::json;
use std::sync::{Arc, Mutex};

const DAY: i64 = 86_400_000;

/// A provider that answers each question by its wording; `mode` changes the drafts.
async fn provider(mode: Arc<Mutex<&'static str>>) -> std::path::PathBuf {
    let router = Router::new()
        .route("/v1/chat/completions", post(|State(mode): State<Arc<Mutex<&'static str>>>, Json(input): Json<Value>| async move {
            let system = input["messages"][0]["content"].as_str().unwrap_or_default().to_owned();
            let user = input["messages"][1]["content"].as_str().unwrap_or_default().to_owned();
            let mode = *mode.lock().unwrap();
            let answer = if system.contains("how they took the answer") {
                if user.contains("wrong") { json!({"outcome":"corrected","confidence":0.95}) } else { json!({"outcome":"accepted","confidence":0.9}) }
            } else if system.contains("kept a note in its memory") {
                json!({"kind":"customer","lasting":true,"private":false,"instruction":false,"relations":[],"confidence":0.9})
            } else if system.contains("Which notes does the correction") {
                json!({"wrong":["n1"],"confidence":0.9})
            } else if system.contains("Sort this request") {
                if user.contains("\"job_types\":[]") {
                    json!({"type":"new","name":"Send a payment reminder","description":"Remind a customer about an invoice","confidence":0.9})
                } else {
                    json!({"type":"t1","confidence":0.9})
                }
            } else if system.contains("Does one of its skills already cover it") {
                json!({"covered_by":null,"coverage":"no","confidence":0.9})
            } else if system.contains("You write a skill") {
                let ask = if mode == "personal" { "Ask Meera Nair" } else { "Ask a person" };
                json!({"name":"payment-reminder","description":"Use when a customer needs a reminder about an unpaid invoice",
                    "steps":format!("1. Find the invoice in the accounts app.\n2. Send the reminder to the billing contact.\n3. {ask} before offering a discount."),
                    "why":"Reminders came up every day."})
            } else if system.contains("Two answers to the same request") {
                json!({"verdict": if mode == "worse" { "worse" } else { "same" },"note":"Both send the reminder.","confidence":0.9})
            } else if system.contains("Follow this skill") {
                return Json(json!({"choices":[{"message":{"content":"I would send the reminder to the billing contact."}}],"usage":{"prompt_tokens":50,"completion_tokens":10}}));
            } else {
                json!({})
            };
            Json(json!({"choices":[{"message":{"content":answer.to_string()}}],"usage":{"prompt_tokens":100,"completion_tokens":20}}))
        }))
        .with_state(mode);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    let dir = std::env::temp_dir().join(format!("evolution-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("key"), "fixture-key").unwrap();
    std::fs::write(dir.join("providers.json"),
        json!([{"provider":"openai","base_url":format!("http://{address}/v1"),"key_file":dir.join("key")}]).to_string()).unwrap();
    dir.join("providers.json")
}

async fn count(db: &PgStore, sql: &str) -> i64 {
    query(&db.db, &format!("SELECT ({sql})::bigint AS n"), vec![])
        .await
        .unwrap()
        .unwrap()
        .try_get("", "n")
        .unwrap()
}

/// Makes the agent's earlier reviews a month old, so the next round reviews again.
async fn due_again(db: &PgStore) {
    execute(
        &db.db,
        "UPDATE relay_skill_reviews SET ran_ms=ran_ms-2592000000",
        vec![],
    )
    .await
    .unwrap();
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn routine_work_becomes_a_skill_the_agent_created_once_a_person_approves() {
    let mut db = fixture().await;
    db.pooled = false;
    let t = team(&db, "review").await;
    let mode = Arc::new(Mutex::new("good"));
    let models = Models {
        file: Some(provider(mode.clone()).await),
        classifier: Some(("openai".into(), "fixture-classifier".into())),
        learning: Some(("openai".into(), "fixture-writer".into())),
    };
    let acme = note(&db, &t, "Acme reminders go to accounts@.").await;
    let mut jobs = vec![];
    for n in 0..10 {
        jobs.push(
            answered(
                &db,
                &t,
                &format!("Remind Acme about invoice {n}"),
                "Sent the reminder to accounts@.",
            )
            .await,
        );
    }
    super::notes::record_used(
        &db.db,
        &t.w,
        &jobs[0].reply,
        std::slice::from_ref(&acme),
        "given",
    )
    .await
    .unwrap();
    reply_to(
        &db,
        &t,
        &jobs[0],
        "That's wrong, Acme reminders go to Priya now",
    )
    .await;

    // Seven hours later: outcomes, the correction holding its note, job types, and a first review.
    let later = now() * 1000 + 7 * 3600 * 1000;
    round(&db, &models, later).await;
    assert_eq!(
        count(&db, "SELECT count(*) FROM relay_request_outcomes").await,
        10
    );
    assert_eq!(count(&db, "SELECT count(*) FROM relay_request_outcomes WHERE outcome='corrected' AND source='classifier'").await, 1);
    assert_eq!(
        field(&db, &acme, "waiting_reason").await,
        "corrected",
        "the note the correction showed wrong is held"
    );
    assert_eq!(count(&db, "SELECT count(*) FROM relay_job_types").await, 1);
    assert_eq!(
        count(&db, "SELECT count(*) FROM relay_request_job_types").await,
        10
    );
    let improving = db.agent_improving(&t.owner, &t.w, &t.agent).await.unwrap();
    assert_eq!(
        improving["reviews"][0]["result"],
        "Checked 10 requests in 1 job types: no new skill needed (no work came up often enough for a skill)."
    );
    // One outcome read, one correction, one note check, ten requests sorted.
    assert_eq!(
        count(
            &db,
            "SELECT count(*) FROM relay_model_usage WHERE assignment='classify'"
        )
        .await,
        13
    );

    // The same job on four different days is routine: the next review proposes a skill.
    execute(&db.db, "UPDATE relay_request_outcomes o SET decided_ms=decided_ms-(n.k % 4)*86400000 FROM
        (SELECT job,row_number() OVER (ORDER BY job) AS k FROM relay_request_outcomes) n WHERE n.job=o.job", vec![]).await.unwrap();
    due_again(&db).await;
    round(&db, &models, later + DAY).await;
    let proposals = db.skill_proposals(&t.owner, &t.w, &t.agent).await.unwrap();
    assert_eq!(proposals.len(), 1);
    let p = &proposals[0];
    assert_eq!(
        (p["kind"].as_str(), p["name"].as_str(), p["state"].as_str()),
        (Some("new"), Some("payment-reminder"), Some("proposed"))
    );
    assert_eq!(p["job_type"], "Send a payment reminder");
    assert_eq!(p["rehearsals"].as_array().unwrap().len(), 3);
    assert_eq!(p["rehearsals"][0]["verdict"], "same");
    assert!(
        p["evidence"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with("Remind Acme"),
        "the owner reads the channel"
    );
    assert!(
        p["files"][0]["text"]
            .as_str()
            .unwrap()
            .contains("name: payment-reminder")
    );
    assert!(
        db.skill_proposals(&t.member, &t.w, &t.agent).await.is_err(),
        "only its owners and admins"
    );

    // Approved twice, it is one skill on the agent, created by it.
    let decision = |approve| super::proposals::SkillDecision {
        approve,
        files: None,
    };
    let first = db
        .decide_skill_proposal(
            &t.owner,
            &t.w,
            &t.agent,
            p["id"].as_str().unwrap(),
            &decision(true),
        )
        .await
        .unwrap();
    let again = db
        .decide_skill_proposal(
            &t.owner,
            &t.w,
            &t.agent,
            p["id"].as_str().unwrap(),
            &decision(true),
        )
        .await
        .unwrap();
    assert_eq!(first["skill"], again["skill"]);
    assert!(
        db.decide_skill_proposal(
            &t.owner,
            &t.w,
            &t.agent,
            p["id"].as_str().unwrap(),
            &decision(false)
        )
        .await
        .is_err()
    );
    let skill = db
        .skill(&t.owner, &t.w, first["skill"].as_str().unwrap(), None)
        .await
        .unwrap();
    assert_eq!(skill["created_by_agent"], "Pool agent");
    assert_eq!(skill["versions"][0]["agent"], "Pool agent");
    assert_eq!(skill["owner_id"], t.owner.id.as_str());
    let on_agent = db.agent_skills(&t.owner, &t.w, &t.agent).await.unwrap();
    assert_eq!(on_agent["skills"][0]["name"], "payment-reminder");
    assert_eq!(on_agent["skills"][0]["created_by_agent"], "Pool agent");
    let improving = db.agent_improving(&t.owner, &t.w, &t.agent).await.unwrap();
    assert_eq!(improving["skills"][0]["name"], "payment-reminder");
    assert_eq!(improving["skills"][0]["job"], "Send a payment reminder");
    assert_eq!(
        db.workspace_learning(&t.owner, &t.w).await.unwrap()["this_week"]["skills_approved"],
        1
    );

    // Not again within three days.
    round(&db, &models, later + DAY + 3_600_000).await;
    assert_eq!(
        count(&db, "SELECT count(*) FROM relay_skill_reviews").await,
        2
    );

    // A draft naming a member is dropped; so is one that rehearses worse.
    execute(&db.db, "DELETE FROM relay_agent_skills", vec![])
        .await
        .unwrap();
    execute(
        &db.db,
        "UPDATE relay_skills SET name='payment-reminder-old'",
        vec![],
    )
    .await
    .unwrap();
    for (draft, why) in [
        ("personal", "did not pass the checks"),
        ("worse", "did worse in rehearsal"),
    ] {
        *mode.lock().unwrap() = draft;
        due_again(&db).await;
        round(&db, &models, later + 5 * DAY).await;
        let result: String = query(
            &db.db,
            "SELECT result FROM relay_skill_reviews ORDER BY ran_ms DESC LIMIT 1",
            vec![],
        )
        .await
        .unwrap()
        .unwrap()
        .try_get("", "result")
        .unwrap();
        assert!(result.contains(why), "{result}");
    }

    // A rejected proposal's job waits 30 days.
    *mode.lock().unwrap() = "good";
    due_again(&db).await;
    round(&db, &models, later + 9 * DAY).await;
    let open = db.skill_proposals(&t.owner, &t.w, &t.agent).await.unwrap();
    let fresh = open.iter().find(|p| p["state"] == "proposed").unwrap();
    db.decide_skill_proposal(
        &t.owner,
        &t.w,
        &t.agent,
        fresh["id"].as_str().unwrap(),
        &decision(false),
    )
    .await
    .unwrap();
    due_again(&db).await;
    round(&db, &models, later + 13 * DAY).await;
    assert!(
        db.skill_proposals(&t.owner, &t.w, &t.agent)
            .await
            .unwrap()
            .iter()
            .all(|p| p["state"] != "proposed")
    );
}

//! Cuckoo Control's store against real PostgreSQL: suspension, caps, spend,
//! settings, provider switches and the change log.
use super::super::tests::*;
use super::*;

fn op(user: &User) -> Operator {
    Operator {
        user: user.clone(),
        via: Via::Web,
    }
}

async fn priced_call(db: &PgStore, w: &str, input: i64, output: i64) {
    db.record_model_call(&crate::identity::postgres::ModelCall {
        workspace: w,
        assignment: "a1",
        agent: "atlas",
        job: None,
        provider: "fireworks",
        model: "kimi-k2",
        requested_model: None,
        input_tokens: input,
        output_tokens: output,
        latency_ms: Some(900),
        ok: true,
        fell_back: false,
    })
    .await
    .unwrap();
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn suspension_refuses_members_pauses_agents_and_lifts_cleanly() {
    let db = fixture().await;
    let operator = user(&db, "control-operator").await;
    let owner = user(&db, "control-owner").await;
    let w = workspace(&db, &owner).await;
    db.agents_pause_state(&owner, &w).await.unwrap();
    db.operator_suspend(&op(&operator), &w, "Payment chargeback")
        .await
        .unwrap();
    // Every member is refused, and the workspace list says why it cannot open.
    assert!(db.agents_pause_state(&owner, &w).await.is_err());
    let listed = db.snapshot(&owner).await.unwrap();
    assert_eq!(listed["workspaces"][0]["suspended"], true);
    assert!(
        crate::identity::postgres::settings::pause::paused(&db.db, &w)
            .await
            .unwrap()
    );
    assert_eq!(
        spend::refusal(&db, &w).await.unwrap().as_deref(),
        Some("This workspace is suspended by Cuckoo")
    );
    assert!(
        db.operator_suspend(&op(&operator), &w, "Again please")
            .await
            .unwrap_err()
            .to_string()
            .contains("already suspended")
    );
    let detail = db.operator_workspace(&w).await.unwrap();
    assert_eq!(detail["status"], "suspended");
    assert_eq!(detail["suspended_note"], "Payment chargeback");
    db.operator_resume(&op(&operator), &w, "Payment arrived")
        .await
        .unwrap();
    let state = db.agents_pause_state(&owner, &w).await.unwrap();
    assert_eq!(state["paused"], false, "suspension's own pause is lifted");
    let log = db.operator_log(Some("workspaces"), None, 50).await.unwrap();
    let summaries: Vec<_> = log["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["summary"].as_str().unwrap().to_owned())
        .collect();
    assert!(summaries[0].starts_with("lifted the suspension"));
    assert!(summaries[1].starts_with("suspended Test business"));
    assert_eq!(log["entries"][1]["reason"], "Payment chargeback");
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn an_admins_own_pause_survives_a_suspension() {
    let db = fixture().await;
    let operator = user(&db, "control-operator-2").await;
    let owner = user(&db, "control-owner-2").await;
    let w = workspace(&db, &owner).await;
    db.pause_agents(&owner, &w, &Default::default())
        .await
        .unwrap();
    db.operator_suspend(&op(&operator), &w, "Abuse report")
        .await
        .unwrap();
    db.operator_resume(&op(&operator), &w, "Report was wrong")
        .await
        .unwrap();
    assert_eq!(
        db.agents_pause_state(&owner, &w).await.unwrap()["paused"],
        true
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn calls_are_priced_at_call_time_and_caps_and_ceilings_refuse_new_work() {
    let db = fixture().await;
    let operator = user(&db, "spend-operator").await;
    let owner = user(&db, "spend-owner").await;
    let w = workspace(&db, &owner).await;
    // Before a price exists, a call is recorded unpriced.
    priced_call(&db, &w, 1_000_000, 0).await;
    db.operator_set_model(
        &op(&operator),
        "fireworks",
        "kimi-k2",
        true,
        0.60,
        2.50,
        "Fireworks price list",
    )
    .await
    .unwrap();
    priced_call(&db, &w, 1_000_000, 500_000).await;
    let spend = db.operator_spend(30).await.unwrap();
    assert_eq!(spend["month"], 1.85);
    assert_eq!(spend["by_model"][0]["unpriced_calls"], 1);
    assert_eq!(spend["by_workspace"][0]["cost"], 1.85);
    assert_eq!(spend["by_workspace"][0]["top_agent"]["name"], "atlas");
    assert_eq!(spend::refusal(&db, &w).await.unwrap(), None);
    db.operator_set_cap(&op(&operator), &w, Some(1.0), "Trial limit")
        .await
        .unwrap();
    assert!(
        spend::refusal(&db, &w)
            .await
            .unwrap()
            .unwrap()
            .contains("monthly AI spending limit of $1.00")
    );
    let overview = db.operator_overview().await.unwrap();
    assert!(
        overview["alerts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["kind"] == "cap" && a["level"] == "bad")
    );
    // Back to the platform default, then a platform-wide daily ceiling.
    db.operator_set_cap(&op(&operator), &w, None, "Paid plan")
        .await
        .unwrap();
    db.operator_set_setting(
        &op(&operator),
        settings::DEFAULT_CAP,
        &json!(5),
        "Default for trials",
    )
    .await
    .unwrap();
    assert_eq!(spend::refusal(&db, &w).await.unwrap(), None);
    db.operator_set_setting(
        &op(&operator),
        settings::DAILY_CEILING,
        &json!("1.50"),
        "Budget this week",
    )
    .await
    .unwrap();
    assert!(
        spend::refusal(&db, &w)
            .await
            .unwrap()
            .unwrap()
            .contains("today's AI spending limit")
    );
    let detail = db.operator_workspace(&w).await.unwrap();
    assert_eq!(detail["spend"], 1.85);
    assert_eq!(detail["cap"], 5.0);
    assert_eq!(detail["own_cap"], false);
    assert_eq!(detail["calls_7d"].as_array().unwrap().len(), 7);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn pausing_sign_up_keeps_the_code_for_later() {
    let db = fixture().await;
    let operator = user(&db, "signup-operator").await;
    let owner = user(&db, "signup-owner").await;
    let code = db
        .issue_workspace_code(&operator, "Bakery", 14)
        .await
        .unwrap()["code"]
        .as_str()
        .unwrap()
        .to_owned();
    db.operator_set_setting(
        &op(&operator),
        settings::SIGNUP_PAUSED,
        &json!("true"),
        "Capacity incident",
    )
    .await
    .unwrap();
    let refused = db
        .create_with_code(&owner, "Bakery", &uuid::Uuid::new_v4().to_string(), &code)
        .await;
    assert!(
        refused
            .unwrap_err()
            .to_string()
            .contains("not opening new workspaces")
    );
    db.operator_set_setting(
        &op(&operator),
        settings::SIGNUP_PAUSED,
        &json!(false),
        "Incident over",
    )
    .await
    .unwrap();
    db.create_with_code(&owner, "Bakery", &uuid::Uuid::new_v4().to_string(), &code)
        .await
        .unwrap();
    let all = db.operator_settings().await.unwrap();
    let paused = all["settings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["key"] == settings::SIGNUP_PAUSED)
        .unwrap();
    assert_eq!(paused["value"], false);
    assert_eq!(
        paused["history"][0]["summary"],
        "changed signup.paused from true to false"
    );
    assert!(
        db.operator_set_setting(
            &op(&operator),
            "signup.paused",
            &json!("maybe"),
            "Unsure now"
        )
        .await
        .is_err()
    );
    assert!(
        db.operator_set_setting(&op(&operator), "nope", &json!(1), "Typo here")
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn provider_switches_are_recorded_and_failed_calls_count_as_errors() {
    let db = fixture().await;
    let operator = user(&db, "switch-operator").await;
    let owner = user(&db, "switch-owner").await;
    let w = workspace(&db, &owner).await;
    db.operator_switch_provider(&op(&operator), "fireworks", false, "Fireworks incident")
        .await
        .unwrap();
    assert_eq!(
        db.model_provider_off("fireworks").await.unwrap().as_deref(),
        Some("Fireworks incident")
    );
    assert_eq!(db.model_provider_off("openai").await.unwrap(), None);
    for ok in [true, false, false] {
        db.record_model_call(&crate::identity::postgres::ModelCall {
            workspace: &w,
            assignment: "a",
            agent: "atlas",
            job: Some("reply-1"),
            provider: "fireworks",
            model: "kimi-k2",
            requested_model: None,
            input_tokens: 10,
            output_tokens: 2,
            latency_ms: Some(1200),
            ok,
            fell_back: false,
        })
        .await
        .unwrap();
    }
    let files = [providers::FileProvider {
        name: "fireworks".into(),
        base_url: "https://api.fireworks.ai/inference/v1".into(),
    }];
    let listed = db.operator_providers(&files).await.unwrap();
    let fireworks = &listed["providers"][0];
    assert_eq!(fireworks["source"], "deployment");
    assert_eq!(fireworks["enabled"], false);
    assert_eq!(fireworks["health"]["calls"], 3);
    assert_eq!(fireworks["health"]["errors"], 2);
    // Activity's per-request model use still counts only answered calls.
    assert_eq!(db.job_model_usage(&w, "reply-1").await.unwrap()["calls"], 1);
    let overview = db.operator_overview().await.unwrap();
    assert!(
        overview["alerts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["kind"] == "provider_off")
    );
    db.operator_switch_provider(&op(&operator), "fireworks", true, "Incident resolved")
        .await
        .unwrap();
    assert_eq!(db.model_provider_off("fireworks").await.unwrap(), None);
    let log = db.operator_log(Some("providers"), None, 1).await.unwrap();
    assert_eq!(
        log["entries"][0]["summary"],
        "turned model provider fireworks back on"
    );
    assert_eq!(log["more"], true);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn the_change_log_is_append_only_and_needs_reasons() {
    let db = fixture().await;
    let operator = user(&db, "log-operator").await;
    assert!(reason("ok").is_err());
    assert!(reason("line\nbreak").is_err());
    assert_eq!(
        reason("  Scheduled rotation  ").unwrap(),
        "Scheduled rotation"
    );
    db.operator_record(
        &op(&operator),
        "codes",
        "code_issued",
        "Bakery",
        "issued a code",
        "Demo call",
    )
    .await
    .unwrap();
    assert!(
        db.operator_record(&op(&operator), "nowhere", "x", "y", "z", "Some reason")
            .await
            .is_err()
    );
    // The runtime role cannot rewrite history.
    assert!(
        execute(
            &db.db,
            "UPDATE relay_operator_log SET reason='changed'",
            vec![]
        )
        .await
        .is_err()
    );
    assert!(
        execute(&db.db, "DELETE FROM relay_operator_log", vec![])
            .await
            .is_err()
    );
    let found = db
        .operator_workspaces("nothing-matches-this")
        .await
        .unwrap();
    assert_eq!(found["workspaces"].as_array().unwrap().len(), 0);
}

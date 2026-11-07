//! Cuckoo Control's store against real PostgreSQL: suspension, the daily
//! ceiling, spend from the gateway's charges and costs, settings, provider
//! switches and keys, and the change log.
use super::super::tests::*;
use super::*;

fn op(user: &User) -> Operator {
    Operator {
        user: user.clone(),
        via: Via::Web,
    }
}

/// One answered call as the gateway saves it (schema 88), in micro-dollars.
async fn call(db: &PgStore, w: &str, provider: &str, charge: i64, cost: i64) {
    db.record_model_usage(&crate::identity::postgres::ModelCall {
        workspace: w,
        assignment: "a1",
        agent: "atlas",
        job: Some("reply-1"),
        provider,
        model: "kimi-k2",
        provider_model: "accounts/fireworks/models/kimi-k2",
        attempts: 1,
        input_tokens: 1000,
        output_tokens: 100,
        person: None,
        charge_micro: charge,
        cost_micro: cost,
        latency_ms: Some(900),
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
    assert_eq!(
        db.snapshot(&owner).await.unwrap()["workspaces"][0]["suspended"],
        true
    );
    assert!(
        crate::identity::postgres::settings::pause::paused(&db.db, &w)
            .await
            .unwrap()
    );
    assert_eq!(
        spend::refusal(&db, &w).await.unwrap().as_deref(),
        Some("This workspace is suspended by Cuckoo")
    );
    let again = db
        .operator_suspend(&op(&operator), &w, "Again please")
        .await;
    assert!(again.unwrap_err().to_string().contains("already suspended"));
    let detail = db.operator_workspace(&w).await.unwrap();
    assert_eq!(detail["status"], "suspended");
    assert_eq!(detail["suspended_note"], "Payment chargeback");
    db.operator_resume(&op(&operator), &w, "Payment arrived")
        .await
        .unwrap();
    assert_eq!(
        db.agents_pause_state(&owner, &w).await.unwrap()["paused"],
        false,
        "suspension's own pause is lifted"
    );
    let log = db.operator_log(Some("workspaces"), None, 50).await.unwrap();
    assert!(
        log["entries"][0]["summary"]
            .as_str()
            .unwrap()
            .starts_with("lifted the suspension")
    );
    assert!(
        log["entries"][1]["summary"]
            .as_str()
            .unwrap()
            .starts_with("suspended Test business")
    );
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
async fn spend_shows_charge_cost_and_margin_and_the_ceiling_stops_calls() {
    let db = fixture().await;
    let operator = user(&db, "spend-operator").await;
    let owner = user(&db, "spend-owner").await;
    let w = workspace(&db, &owner).await;
    call(&db, &w, "fireworks", 1_500_000, 900_000).await;
    call(&db, &w, "openai", 500_000, 600_000).await;
    let spend = db.operator_spend(30).await.unwrap();
    assert_eq!(
        spend["month"],
        json!({"charge": 2.0, "cost": 1.5, "margin": 0.5})
    );
    assert_eq!(spend["today"]["cost"], 1.5);
    assert_eq!(spend["by_workspace"][0]["margin"], 0.5);
    assert_eq!(spend["by_workspace"][0]["top_agent"]["name"], "atlas");
    assert_eq!(spend["daily"].as_array().unwrap().len(), 30);
    let detail = db.operator_workspace(&w).await.unwrap();
    assert_eq!(
        (detail["charge"].clone(), detail["cost"].clone()),
        (json!(2.0), json!(1.5))
    );
    assert_eq!(detail["budget"], Value::Null);
    assert_eq!(detail["calls_7d"].as_array().unwrap().len(), 7);
    assert_eq!(spend::refusal(&db, &w).await.unwrap(), None);
    // The ceiling is on what providers charge us, across every workspace.
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
    let overview = db.operator_overview().await.unwrap();
    assert!(
        overview["alerts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["kind"] == "ceiling" && a["level"] == "bad")
    );
    assert_eq!(overview["today"], json!({"charge": 2.0, "cost": 1.5}));
    db.operator_set_setting(
        &op(&operator),
        settings::DAILY_CEILING,
        &json!(null),
        "Lifted",
    )
    .await
    .unwrap();
    assert_eq!(spend::refusal(&db, &w).await.unwrap(), None);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn workspace_budgets_near_their_limit_are_alerts() {
    let db = fixture().await;
    let owner = user(&db, "budget-owner").await;
    let w = workspace(&db, &owner).await;
    execute(&db.db, "INSERT INTO relay_model_budgets(workspace,agent,monthly_micro,warn_percent,updated_by,updated_ms) VALUES($1,'',2000000,80,$2,0)",
        vec![w.clone().into(), owner.id.clone().into()]).await.unwrap();
    call(&db, &w, "fireworks", 1_700_000, 1_000_000).await;
    let overview = db.operator_overview().await.unwrap();
    let alert = overview["alerts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["kind"] == "budget")
        .cloned()
        .unwrap();
    assert_eq!(alert["level"], "warn");
    assert_eq!(
        alert["title"],
        "Test business has used 85% of its monthly budget"
    );
    assert_eq!(db.operator_workspace(&w).await.unwrap()["budget"], 2.0);
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
        db.operator_set_setting(
            &op(&operator),
            "workspace.default_cap_usd",
            &json!(1),
            "Gone now"
        )
        .await
        .is_err()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn provider_switches_keys_and_failures_show_in_the_provider_list() {
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
    call(&db, &w, "fireworks", 10, 5).await;
    db.record_model_failure(&w, "fireworks", "kimi-k2", 503)
        .await
        .unwrap();
    db.record_model_failure(&w, "fireworks", "kimi-k2", 0)
        .await
        .unwrap();
    // A file provider whose key is kept in the console.
    db.operator_store_key(
        &op(&operator),
        "fireworks",
        b"sealed".to_vec(),
        "3f9a",
        "Key moved to the console",
    )
    .await
    .unwrap();
    let files = [providers::FileProvider {
        name: "fireworks".into(),
        base_url: "https://api.fireworks.ai/inference/v1".into(),
    }];
    let routes = [providers::RouteUse {
        model: "kimi-k2".into(),
        provider: "fireworks".into(),
        provider_model: "accounts/fireworks/models/kimi-k2".into(),
        priority: 1,
    }];
    let listed = db.operator_providers(&files, &routes).await.unwrap();
    let fireworks = &listed["providers"][0];
    assert_eq!(
        (
            fireworks["source"].clone(),
            fireworks["key_in_console"].clone()
        ),
        (json!("deployment"), json!(true))
    );
    assert_eq!(fireworks["key_hint"], "3f9a");
    assert_eq!(fireworks["enabled"], false);
    assert_eq!(
        fireworks["health"],
        json!({"calls": 1, "errors": 2, "p95_ms": 900})
    );
    assert_eq!(fireworks["routes"][0]["model"], "kimi-k2");
    assert_eq!(
        db.model_provider_stored("fireworks")
            .await
            .unwrap()
            .unwrap()
            .base_url,
        None
    );
    let overview = db.operator_overview().await.unwrap();
    assert!(
        overview["alerts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["kind"] == "provider_off")
    );
    assert_eq!(overview["errors_1h"], 2);
    db.operator_switch_provider(&op(&operator), "fireworks", true, "Incident resolved")
        .await
        .unwrap();
    assert_eq!(db.model_provider_off("fireworks").await.unwrap(), None);
    db.operator_remove_provider(&op(&operator), "fireworks", "Back to the key file")
        .await
        .unwrap();
    let log = db.operator_log(Some("providers"), None, 1).await.unwrap();
    assert_eq!(
        log["entries"][0]["summary"],
        "removed fireworks's API key from Cuckoo Control; it uses its key file again"
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
        db.operator_record(&op(&operator), "routing", "x", "y", "z", "Some reason")
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

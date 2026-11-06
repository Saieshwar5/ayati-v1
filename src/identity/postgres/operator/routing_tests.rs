//! Routes and the model catalog against real PostgreSQL.
use super::super::tests::*;
use super::models::{Route, Target};
use super::*;

fn op(user: &User) -> Operator {
    Operator {
        user: user.clone(),
        via: Via::Cli,
    }
}

fn route(main_percent: i32) -> Route {
    Route {
        model: "kimi-k2".into(),
        main: Target {
            provider: "fireworks".into(),
            model: "accounts/fireworks/models/kimi-k2".into(),
        },
        second: Some(Target {
            provider: "openai".into(),
            model: "gpt-5-mini".into(),
        }),
        main_percent,
        timeout_seconds: 30,
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn routes_save_with_a_readable_history_and_can_be_removed() {
    let db = fixture().await;
    let operator = user(&db, "route-operator").await;
    assert_eq!(db.model_route("kimi-k2").await.unwrap(), None);
    db.operator_set_route(&op(&operator), &route(100), "Start on Fireworks")
        .await
        .unwrap();
    db.operator_set_route(&op(&operator), &route(90), "Try OpenAI with 10%")
        .await
        .unwrap();
    assert_eq!(db.model_route("kimi-k2").await.unwrap(), Some(route(90)));
    let log = db.operator_log(Some("routing"), None, 10).await.unwrap();
    assert_eq!(log["entries"][0]["via"], "cli");
    assert!(
        log["entries"][0]["summary"]
            .as_str()
            .unwrap()
            .starts_with("changed route kimi-k2 → 90%")
    );
    assert!(
        log["entries"][0]["summary"]
            .as_str()
            .unwrap()
            .contains("(was kimi-k2 → 100%")
    );
    let listed = db.operator_routes().await.unwrap();
    assert_eq!(listed["routes"][0]["main_percent"], 90);
    assert_eq!(listed["routes"][0]["traffic"], json!([]));
    db.operator_remove_route(&op(&operator), "kimi-k2", "Back to agent choice")
        .await
        .unwrap();
    assert_eq!(db.model_route("kimi-k2").await.unwrap(), None);
    assert!(
        db.operator_remove_route(&op(&operator), "kimi-k2", "Again")
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn routes_refuse_blocked_models_and_impossible_splits() {
    let db = fixture().await;
    let operator = user(&db, "route-operator-2").await;
    db.operator_set_model(
        &op(&operator),
        "openai",
        "gpt-5-mini",
        false,
        0.25,
        2.0,
        "Too costly now",
    )
    .await
    .unwrap();
    assert!(db.model_blocked("openai", "gpt-5-mini").await.unwrap());
    assert!(!db.model_blocked("openai", "not-in-catalog").await.unwrap());
    let blocked = db
        .operator_set_route(&op(&operator), &route(90), "Try it")
        .await;
    assert!(
        blocked
            .unwrap_err()
            .to_string()
            .contains("blocked in the model catalog")
    );
    let mut lonely = route(90);
    lonely.second = None;
    assert!(
        db.operator_set_route(&op(&operator), &lonely, "Only one")
            .await
            .is_err()
    );
    let mut same = route(50);
    same.second = Some(same.main.clone());
    assert!(
        db.operator_set_route(&op(&operator), &same, "Same twice")
            .await
            .is_err()
    );
    let mut slow = route(100);
    slow.timeout_seconds = 1;
    assert!(
        db.operator_set_route(&op(&operator), &slow, "Too quick")
            .await
            .is_err()
    );
    let mut bad = route(100);
    bad.main.provider = "Fire Works".into();
    assert!(
        db.operator_set_route(&op(&operator), &bad, "Bad name")
            .await
            .is_err()
    );
}

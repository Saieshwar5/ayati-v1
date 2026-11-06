//! MG4: the operator view adds up every workspace's traffic, money and failures.
use super::ModelCall;
use super::tests::*;

fn call<'a>(w: &'a str, provider: &'a str, attempts: usize, latency: i64) -> ModelCall<'a> {
    ModelCall {
        workspace: w,
        assignment: "fixture-assignment",
        agent: "agent",
        job: None,
        provider,
        model: "llama-70b",
        provider_model: "llama",
        attempts,
        input_tokens: 100,
        output_tokens: 50,
        person: None,
        charge_micro: 1_000,
        cost_micro: 600,
        latency_ms: Some(latency),
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn operators_see_traffic_money_speed_and_failures_across_workspaces() {
    let db = fixture().await;
    let a = user(&db, "ops-a").await;
    let b = user(&db, "ops-b").await;
    let (wa, wb) = (workspace(&db, &a).await, workspace(&db, &b).await);
    for (w, provider, attempts, latency) in [
        (&wa, "fireworks", 1, 100),
        (&wa, "fireworks", 1, 300),
        (&wb, "fireworks", 2, 900),
        (&wb, "openrouter", 1, 200),
    ] {
        db.record_model_usage(&call(w, provider, attempts, latency))
            .await
            .unwrap();
    }
    // A library call is the controller's own work, not provider traffic for agents.
    db.record_model_usage(&ModelCall {
        assignment: "library",
        ..call(&wa, "fireworks", 1, 5)
    })
    .await
    .unwrap();
    for status in [503, 503, 0] {
        db.record_model_failure(&wb, "together", "llama-70b", status)
            .await
            .unwrap();
    }
    let report = db.model_operations(0).await.unwrap();
    let fireworks = &report["providers"][0];
    assert_eq!(fireworks["key"], "fireworks");
    assert_eq!(
        fireworks["calls"], 3,
        "both workspaces, without the Library call"
    );
    assert_eq!(fireworks["after_fallback"], 1);
    assert_eq!(fireworks["tokens"], 450);
    assert_eq!(fireworks["charge_usd"], 0.003);
    assert_eq!(fireworks["cost_usd"], 0.0018);
    assert!((fireworks["margin_usd"].as_f64().unwrap() - 0.0012).abs() < 1e-9);
    assert_eq!(fireworks["latency_avg_ms"], 433);
    assert_eq!(fireworks["latency_p95_ms"], 900);
    assert_eq!(report["models"][0]["calls"], 4);
    let failures = report["failures"].as_array().unwrap();
    assert_eq!(
        (
            &failures[0]["provider"],
            &failures[0]["status"],
            &failures[0]["count"]
        ),
        (
            &serde_json::json!("together"),
            &serde_json::json!(503),
            &serde_json::json!(2)
        )
    );
    assert_eq!(failures.len(), 2, "503s and unreachable are counted apart");
    // A later window leaves everything out.
    let later = db.model_operations(i64::MAX / 2).await.unwrap();
    assert_eq!(later["providers"].as_array().unwrap().len(), 0);
}

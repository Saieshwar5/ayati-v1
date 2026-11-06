//! Cuckoo Control over HTTP and the gateway's routing, against real PostgreSQL
//! and loopback model providers.
use crate::identity::postgres::tests::{fixture, user, workspace};
use crate::identity::worker_control::model::Scope;
use crate::identity::worker_control::model_gateway::{Recorder, first_answer};
use crate::identity::worker_control::model_routing::{self, Plan};
use crate::identity::{App, Provider, digest, router, workspaces::Runtimes};
use axum::{
    Router,
    body::{Body, to_bytes},
    http::Request,
};
use serde_json::{Value, json};
use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;
use tower::ServiceExt;

const ORIGIN: &str = "http://127.0.0.1:9898";

async fn app(dir: &std::path::Path, providers: Value) -> Arc<App> {
    let secrets = dir.join("secrets");
    std::fs::write(&secrets, "7f".repeat(32)).unwrap();
    std::fs::set_permissions(&secrets, std::fs::Permissions::from_mode(0o600)).unwrap();
    let file = dir.join("providers.json");
    std::fs::write(&file, providers.to_string()).unwrap();
    let mut config = crate::config::Controller::for_tests(ORIGIN);
    config.platform_admins = vec!["operator@example.test".into()];
    config.secrets_key_file = Some(secrets);
    config.model_providers_file = Some(file);
    Arc::new(App {
        library: Default::default(),
        provider: Provider::test_provider(),
        store: fixture().await,
        config: Arc::new(config),
        runtimes: Runtimes::new(dir.join("workspaces")),
        simulation: Some(crate::identity::simulation::Simulation {
            employees: vec![],
            workspace: String::new(),
            root: dir.into(),
            paused: std::sync::atomic::AtomicBool::new(true),
        }),
    })
}

async fn call(router: &Router, method: &str, path: &str, token: &str, body: Value) -> (u16, Value) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("Host", "127.0.0.1:9898")
                .header("Cookie", format!("ayati_identity={token}"))
                .header("Origin", ORIGIN)
                .header("X-Ayati-Request", "identity")
                .header("Content-Type", "application/json")
                .body(if method == "GET" {
                    Body::empty()
                } else {
                    Body::from(body.to_string())
                })
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status().as_u16();
    let bytes = to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn sign_in(app: &App, name: &str, email: &str) -> crate::identity::User {
    // Simulation sessions skip the WorkOS check; the platform admin rule is the same.
    app.store
        .login(
            &format!("simulation_employee_{name}"),
            email,
            &digest(&format!("{name}-token")),
            "local-simulation",
        )
        .await
        .unwrap()
}

/// A loopback OpenAI-style provider answering with `status`, counting calls.
async fn provider(status: u16) -> (String, Arc<std::sync::atomic::AtomicUsize>) {
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let seen = calls.clone();
    let service = axum::Router::new().route(
        "/v1/chat/completions",
        axum::routing::post(move |axum::Json(body): axum::Json<Value>| {
            let seen = seen.clone();
            async move {
                seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                (
                    axum::http::StatusCode::from_u16(status).unwrap(),
                    axum::Json(json!({"model": body["model"], "choices": [], "usage": {"prompt_tokens": 1000, "completion_tokens": 100}})),
                )
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, service).await.unwrap() });
    (format!("http://127.0.0.1:{}/v1", address.port()), calls)
}

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn only_platform_administrators_reach_cuckoo_control() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path(), json!([])).await;
    let router = router(app.clone());
    sign_in(&app, "member", "member@example.test").await;
    sign_in(&app, "operator", "operator@example.test").await;
    for (method, path) in [
        ("GET", "/api/identity/admin/overview"),
        ("GET", "/api/identity/admin/workspaces"),
        ("GET", "/api/identity/admin/providers"),
        ("POST", "/api/identity/admin/providers/fireworks/switch"),
        ("POST", "/api/identity/admin/settings"),
        ("GET", "/api/identity/admin/log"),
    ] {
        let attempt = if path.ends_with("/settings") {
            json!({"key": "signup.paused", "value": true, "reason": "Trying it"})
        } else {
            json!({"enabled": false, "reason": "Trying it"})
        };
        let (status, body) = call(&router, method, path, "member-token", attempt.clone()).await;
        assert_eq!(status, 403, "{path}");
        assert_eq!(
            body["error"],
            "Only Cuckoo operators can open Cuckoo Control"
        );
        assert_eq!(
            call(&router, method, path, "nobody", attempt).await.0,
            401,
            "{path}"
        );
    }
    let (status, session) = call(
        &router,
        "GET",
        "/api/identity/admin/session",
        "operator-token",
        Value::Null,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(session["environment"], "local");
    assert_eq!(session["can_store_keys"], true);
    // A reason is required for changes.
    let (status, body) = call(
        &router,
        "POST",
        "/api/identity/admin/providers/fireworks/switch",
        "operator-token",
        json!({"enabled": false, "reason": "x"}),
    )
    .await;
    assert_eq!(status, 400);
    assert!(body["error"].as_str().unwrap().contains("Say why"));
}

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn keys_added_in_the_console_are_sealed_and_never_returned() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path(), json!([{"provider": "openai", "base_url": "https://api.openai.com/v1", "key_file": "/nonexistent"}])).await;
    let router = router(app.clone());
    sign_in(&app, "operator", "operator@example.test").await;
    let key = "fw_secret_value_9f3a";
    let (status, body) = call(&router, "POST", "/api/identity/admin/providers", "operator-token",
        json!({"name": "fireworks", "base_url": "https://api.fireworks.ai/inference/v1/", "key": key, "reason": "Main provider"})).await;
    assert_eq!(status, 200, "{body}");
    assert!(!body.to_string().contains(key));
    let fireworks = body["providers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "fireworks")
        .unwrap();
    assert_eq!(fireworks["key_hint"], "9f3a");
    assert_eq!(
        fireworks["base_url"],
        "https://api.fireworks.ai/inference/v1"
    );
    assert!(
        body["providers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["name"] == "openai" && p["source"] == "deployment")
    );
    // A name the deployment already uses, plain HTTP and keys in addresses are refused.
    for (name, url) in [
        ("openai", "https://example.test"),
        ("other", "http://example.test"),
        ("other", "https://u:p@example.test"),
    ] {
        let (status, _) = call(
            &router,
            "POST",
            "/api/identity/admin/providers",
            "operator-token",
            json!({"name": name, "base_url": url, "key": key, "reason": "Should fail"}),
        )
        .await;
        assert_eq!(status, 400, "{name} {url}");
    }
    let stored = app
        .store
        .model_provider_stored("fireworks")
        .await
        .unwrap()
        .unwrap();
    assert!(!String::from_utf8_lossy(&stored.sealed).contains(key));
    let (status, log) = call(
        &router,
        "GET",
        "/api/identity/admin/log?area=providers",
        "operator-token",
        Value::Null,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(log["entries"][0]["via"], "web");
    assert!(!log.to_string().contains(key));
}

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn the_gateway_follows_routes_switches_and_backups() {
    let dir = tempfile::tempdir().unwrap();
    let (main_url, main_calls) = provider(503).await;
    let (second_url, second_calls) = provider(200).await;
    let key_file = dir.path().join("second-key");
    std::fs::write(&key_file, "sk-second").unwrap();
    let app = app(
        dir.path(),
        json!([{"provider": "openai", "base_url": second_url, "key_file": key_file}]),
    )
    .await;
    let router = router(app.clone());
    let operator = sign_in(&app, "operator", "operator@example.test").await;
    let owner = user(&app.store, "gateway-owner").await;
    let w = workspace(&app.store, &owner).await;
    for (path, body) in [
        (
            "/api/identity/admin/providers",
            json!({"name": "fireworks", "base_url": main_url, "key": "fw-key-1234", "reason": "Main provider"}),
        ),
        (
            "/api/identity/admin/models",
            json!({"provider": "openai", "model": "gpt-5-mini", "enabled": true, "input_per_mtok": 1.0, "output_per_mtok": 10.0, "reason": "Price list"}),
        ),
        (
            "/api/identity/admin/routes",
            json!({"model": "kimi-k2", "main_provider": "fireworks", "main_model": "kimi-k2-instruct",
            "second_provider": "openai", "second_model": "gpt-5-mini", "main_percent": 100, "timeout_seconds": 10, "reason": "Fireworks first"}),
        ),
    ] {
        let (status, answer) = call(&router, "POST", path, "operator-token", body).await;
        assert_eq!(status, 200, "{path}: {answer}");
    }
    // A route may only name configured providers.
    let (status, _) = call(
        &router,
        "POST",
        "/api/identity/admin/routes",
        "operator-token",
        json!({"model": "x", "main_provider": "ghost",
        "main_model": "m", "main_percent": 100, "timeout_seconds": 10, "reason": "Unknown one"}),
    )
    .await;
    assert_eq!(status, 400);
    let plan = plan_for(&app, &w, "reply-1").await.unwrap();
    assert_eq!(
        plan.targets
            .iter()
            .map(|t| t.provider.as_str())
            .collect::<Vec<_>>(),
        ["fireworks", "openai"]
    );
    assert_eq!(
        plan.targets[0].key, "fw-key-1234",
        "the sealed key opens for the gateway"
    );
    assert_eq!(plan.targets[1].key, "sk-second");
    // The main provider fails (503): the request moves to the backup and both are recorded.
    let recorder = Recorder {
        store: app.store.clone(),
        scope: Scope {
            executor: "e".into(),
            incarnation: "i".into(),
            slot: "s".into(),
            slot_generation: 0,
            assignment: "assignment-1".into(),
            workspace: w.clone(),
            agent: "atlas".into(),
            chat: "c".into(),
            thread: None,
            reply: "reply-1".into(),
            generation: 1,
        },
        job: Some("reply-1".into()),
        requested: Some("kimi-k2".into()),
    };
    let mut body = json!({"messages": []});
    let Ok((response, index, started)) = first_answer(&plan, &mut body, &recorder).await else {
        panic!("no answer")
    };
    assert_eq!((response.status().as_u16(), index), (200, 1));
    assert_eq!(
        response.json::<Value>().await.unwrap()["model"],
        "gpt-5-mini"
    );
    assert_eq!(
        (
            main_calls.load(std::sync::atomic::Ordering::SeqCst),
            second_calls.load(std::sync::atomic::Ordering::SeqCst)
        ),
        (1, 1)
    );
    recorder
        .save_named("openai", "gpt-5-mini", started, true, true, (1000, 100))
        .await;
    let (_, providers) = call(
        &router,
        "GET",
        "/api/identity/admin/providers",
        "operator-token",
        Value::Null,
    )
    .await;
    let fireworks = providers["providers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "fireworks")
        .unwrap();
    assert_eq!(fireworks["health"]["errors"], 1);
    let (_, spend) = call(
        &router,
        "GET",
        "/api/identity/admin/spend",
        "operator-token",
        Value::Null,
    )
    .await;
    assert_eq!(
        spend["month"], 0.002,
        "1,000 in at $1 + 100 out at $10 per million"
    );
    let (_, overview) = call(
        &router,
        "GET",
        "/api/identity/admin/overview",
        "operator-token",
        Value::Null,
    )
    .await;
    assert_eq!(overview["fell_back_1h"], 1);
    // Switched off: the gateway skips Fireworks entirely.
    let (status, _) = call(
        &router,
        "POST",
        "/api/identity/admin/providers/fireworks/switch",
        "operator-token",
        json!({"enabled": false, "reason": "Fireworks incident"}),
    )
    .await;
    assert_eq!(status, 200);
    let plan = plan_for(&app, &w, "reply-2").await.unwrap();
    assert_eq!(plan.targets.len(), 1);
    assert_eq!(plan.targets[0].provider, "openai");
    // Both off: refused with the operator's note, nothing is sent.
    call(
        &router,
        "POST",
        "/api/identity/admin/providers/openai/switch",
        "operator-token",
        json!({"enabled": false, "reason": "OpenAI incident"}),
    )
    .await;
    let refused = plan_for(&app, &w, "reply-3").await.err().unwrap();
    assert_eq!(
        refused,
        "Cuckoo paused fireworks: Fireworks incident; Cuckoo paused openai: OpenAI incident"
    );
    // A suspended workspace is refused before any provider is considered.
    let by = crate::identity::postgres::operator::Operator {
        user: operator,
        via: crate::identity::postgres::operator::Via::Web,
    };
    app.store
        .operator_suspend(&by, &w, "Chargeback")
        .await
        .unwrap();
    super::super::worker_control::model_routing::forget_limits();
    assert_eq!(
        plan_for(&app, &w, "reply-4").await.err().unwrap(),
        "This workspace is suspended by Cuckoo"
    );
}

/// The gateway's plan for an agent asking for kimi-k2 on openai.
async fn plan_for(app: &App, w: &str, job: &str) -> Result<Plan, String> {
    match model_routing::plan(app, w, "openai", "kimi-k2", job)
        .await
        .unwrap()
    {
        Ok(plan) => Ok(plan),
        Err(model_routing::Refused(message)) => Err(message),
    }
}

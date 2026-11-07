//! Cuckoo Control over HTTP and the gateway's routing, against real PostgreSQL
//! and loopback model providers.
use crate::identity::postgres::tests::{fixture, user, workspace};
use crate::identity::worker_control::model_routing;
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
async fn the_gateway_uses_console_keys_and_honors_switches_and_suspension() {
    let dir = tempfile::tempdir().unwrap();
    let key_file = dir.path().join("openai-key");
    std::fs::write(&key_file, "sk-from-file").unwrap();
    let app = app(
        dir.path(),
        json!([
            {"provider": "fireworks", "base_url": "https://api.fireworks.ai/inference/v1"},
            {"provider": "openai", "base_url": "https://api.openai.com/v1", "key_file": key_file},
        ]),
    )
    .await;
    let router = router(app.clone());
    sign_in(&app, "operator", "operator@example.test").await;
    let owner = user(&app.store, "gateway-owner").await;
    let w = workspace(&app.store, &owner).await;
    // A file provider with no key file has no key until one is kept in the console.
    let missing = model_routing::credentials(&app, "fireworks")
        .await
        .unwrap_err();
    assert!(format!("{missing:#}").contains("has no key"), "{missing:#}");
    let (status, body) = call(
        &router,
        "POST",
        "/api/identity/admin/providers/fireworks/key",
        "operator-token",
        json!({"key": "fw-console-1234", "reason": "Key kept in the console"}),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert!(!body.to_string().contains("fw-console-1234"));
    assert_eq!(
        model_routing::credentials(&app, "fireworks").await.unwrap(),
        (
            "https://api.fireworks.ai/inference/v1".into(),
            "fw-console-1234".into()
        )
    );
    assert_eq!(
        model_routing::credentials(&app, "openai").await.unwrap(),
        ("https://api.openai.com/v1".into(), "sk-from-file".into())
    );
    // Keys only for configured providers; console providers can't shadow the file.
    let (status, _) = call(
        &router,
        "POST",
        "/api/identity/admin/providers/ghost/key",
        "operator-token",
        json!({"key": "gh-key-123456", "reason": "Unknown one"}),
    )
    .await;
    assert_eq!(status, 400);
    // Switched off: its routes are dropped, with the operator's note.
    let (status, _) = call(
        &router,
        "POST",
        "/api/identity/admin/providers/fireworks/switch",
        "operator-token",
        json!({"enabled": false, "reason": "Fireworks incident"}),
    )
    .await;
    assert_eq!(status, 200);
    let (on, paused) = model_routing::switched_on(
        &app.store,
        vec![
            ("fireworks".to_owned(), 1),
            ("openai".to_owned(), 2),
            ("fireworks".to_owned(), 3),
        ],
    )
    .await
    .unwrap();
    assert_eq!(on, vec![("openai".to_owned(), 2)]);
    assert_eq!(
        paused,
        vec!["Cuckoo paused fireworks: Fireworks incident".to_owned()]
    );
    // A suspended workspace is refused before any provider is called.
    model_routing::forget_limits();
    assert_eq!(model_routing::refusal(&app.store, &w).await.unwrap(), None);
    let (status, _) = call(
        &router,
        "POST",
        &format!("/api/identity/admin/workspaces/{w}/suspend"),
        "operator-token",
        json!({"reason": "Chargeback"}),
    )
    .await;
    assert_eq!(status, 200);
    model_routing::forget_limits();
    assert_eq!(
        model_routing::refusal(&app.store, &w)
            .await
            .unwrap()
            .as_deref(),
        Some("This workspace is suspended by Cuckoo")
    );
}

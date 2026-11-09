//! Provider event routes over HTTP (schema 109): the platform admin's Composio
//! setup, and the signed hooks' narrow exemption from browser checks.
use crate::identity::connectors::fake_composio::{FakeComposio, WEBHOOK_SECRET};
use crate::identity::postgres::tests::fixture;
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

const HTTPS: &str = "https://cuckoo.example.test";
const LOCAL: &str = "http://127.0.0.1:9898";

async fn app(dir: &std::path::Path, origin: &str, fake: &FakeComposio, secrets: bool) -> Arc<App> {
    let mut config = crate::config::Controller::for_tests(origin);
    config.platform_admins = vec!["operator@example.test".into()];
    if secrets {
        let file = dir.join("secrets");
        std::fs::write(&file, "7f".repeat(32)).unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        config.secrets_key_file = Some(file);
    }
    config.connectors.composio_key_file = Some(fake.key_file.clone());
    config.connectors.composio_base_url = Some(fake.base.clone());
    let store = fixture().await;
    for (name, email) in [
        ("member", "member@example.test"),
        ("operator", "operator@example.test"),
    ] {
        store
            .login(
                &format!("simulation_employee_{name}"),
                email,
                &digest(&format!("{name}-token")),
                "local-simulation",
            )
            .await
            .unwrap();
    }
    Arc::new(App {
        library: Default::default(),
        provider: Provider::test_provider(),
        store,
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

/// A browser request (cookie, Origin and request header) or, without a token,
/// a provider's (none of them).
async fn call(
    router: &Router,
    origin: &str,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: Vec<u8>,
) -> (u16, Value) {
    let host = origin.split_once("://").unwrap().1;
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header("Host", host);
    if let Some(token) = token {
        request = request
            .header("Cookie", format!("ayati_identity={token}"))
            .header("Origin", origin)
            .header("X-Ayati-Request", "identity");
    }
    let response = router
        .clone()
        .oneshot(
            request
                .header("Content-Type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status().as_u16();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL"]
async fn only_platform_admins_set_up_composio_events_and_the_secret_stays_sealed() {
    let dir = tempfile::tempdir().unwrap();
    let fake = FakeComposio::start().await;
    let path = "/api/identity/admin/connector-events/composio";
    // Without a public HTTPS address, or without the secrets key, it refuses clearly.
    let local = router(app(dir.path(), LOCAL, &fake, true).await);
    let (status, body) = call(&local, LOCAL, "POST", path, Some("operator-token"), vec![]).await;
    assert_eq!(status, 400);
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .contains("AYATI_PUBLIC_ORIGIN"),
        "{body}"
    );
    let keyless = router(app(dir.path(), HTTPS, &fake, false).await);
    let (status, body) = call(
        &keyless,
        HTTPS,
        "POST",
        path,
        Some("operator-token"),
        vec![],
    )
    .await;
    assert_eq!(status, 400);
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .contains("AYATI_SECRETS_KEY_FILE"),
        "{body}"
    );
    assert!(fake.state.lock().unwrap().webhook.is_none());
    // Only platform admins, and only from the browser with its checks.
    let app = app(dir.path(), HTTPS, &fake, true).await;
    let router = router(app.clone());
    assert_eq!(
        call(&router, HTTPS, "POST", path, Some("member-token"), vec![])
            .await
            .0,
        403
    );
    assert_eq!(
        call(&router, HTTPS, "POST", path, None, vec![]).await.0,
        403
    );
    let (status, before) = call(&router, HTTPS, "GET", path, Some("operator-token"), vec![]).await;
    assert_eq!((status, before["set_up"].as_bool()), (200, Some(false)));
    // Set up: Composio delivers both event types to this server's one hook.
    let (status, saved) = call(&router, HTTPS, "POST", path, Some("operator-token"), vec![]).await;
    assert_eq!(status, 200, "{saved}");
    assert_eq!(saved["set_up"], true);
    let webhook = fake.state.lock().unwrap().webhook.clone().unwrap();
    assert_eq!(webhook["webhook_url"], format!("{HTTPS}/hooks/composio"));
    assert_eq!(
        webhook["enabled_events"],
        json!([
            "composio.trigger.message",
            "composio.connected_account.expired"
        ])
    );
    assert_eq!(webhook["version"], "V3");
    // Again: the project's existing subscription is updated, not duplicated.
    assert_eq!(
        call(&router, HTTPS, "POST", path, Some("operator-token"), vec![])
            .await
            .0,
        200
    );
    // The secret is stored sealed and never returned.
    let (_, after) = call(&router, HTTPS, "GET", path, Some("operator-token"), vec![]).await;
    assert!(!format!("{saved}{after}").contains(WEBHOOK_SECRET));
    let sealed = app
        .store
        .connector_event_secret("composio")
        .await
        .unwrap()
        .unwrap();
    assert!(!String::from_utf8_lossy(&sealed).contains(WEBHOOK_SECRET));
    // The signed hooks skip the browser checks and reach their handlers; the
    // Composio provider isn't registered in tests, so intake is unavailable.
    assert_eq!(
        call(
            &router,
            HTTPS,
            "POST",
            "/hooks/composio",
            None,
            b"{}".to_vec()
        )
        .await
        .0,
        503
    );
    let pipedream = format!("/hooks/pipedream/{}", uuid::Uuid::new_v4());
    assert_eq!(
        call(&router, HTTPS, "POST", &pipedream, None, b"{}".to_vec())
            .await
            .0,
        404
    );
    assert_eq!(
        call(
            &router,
            HTTPS,
            "POST",
            "/hooks/pipedream/not-a-subscription",
            None,
            vec![]
        )
        .await
        .0,
        403
    );
    assert_eq!(
        call(
            &router,
            HTTPS,
            "POST",
            "/hooks/composio",
            None,
            vec![b' '; 64001]
        )
        .await
        .0,
        413
    );
}

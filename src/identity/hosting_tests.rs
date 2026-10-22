use super::*;
use tower::ServiceExt;
fn app() -> Shared {
    Arc::new(App {
        library: Default::default(),
        provider: Provider::test_provider(),
        store: Store::open(Path::new(":memory:")).unwrap().into(),
        origin: "https://workspace.example.test".into(),
        pending: Default::default(),
        runtimes: workspaces::Runtimes::new(
            std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()),
        ),
        simulation: None,
    })
}
#[tokio::test]
async fn hosted_login_uses_public_callback_secure_cookies_and_strict_request_authority() {
    let app = app();
    let router = router(app.clone());
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/auth/login")
                .header("host", "workspace.example.test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let url = reqwest::Url::parse(response.headers()[header::LOCATION].to_str().unwrap()).unwrap();
    assert!(
        url.query_pairs().any(
            |(k, v)| k == "redirect_uri" && v == "https://workspace.example.test/auth/callback"
        )
    );
    for name in [FLOW, SESSION] {
        for age in [0, 600] {
            let value = set_cookie(&app, name, "value", age);
            let value = value.to_str().unwrap();
            assert!(
                value.contains("; Secure")
                    && value.contains("HttpOnly")
                    && value.contains("SameSite=Lax")
            );
            assert!(!value.contains("Domain="));
        }
    }
    for (host, origin, forwarded, expected) in [
        (
            "workspace.example.test",
            "https://workspace.example.test",
            "http",
            200,
        ),
        ("evil.test", "https://workspace.example.test", "https", 403),
        (
            "workspace.example.test",
            "http://workspace.example.test",
            "https",
            403,
        ),
        ("workspace.example.test", "https://evil.test", "https", 403),
    ] {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/identity/logout")
                    .header("host", host)
                    .header("origin", origin)
                    .header("x-forwarded-proto", forwarded)
                    .header("x-forwarded-host", "workspace.example.test")
                    .header("x-ayati-request", "identity")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), expected);
    }
}
#[tokio::test]
async fn health_endpoints_allow_only_read_only_unscoped_probes() {
    let router = router(app());
    for (path, method, expected) in [
        ("/health/live", "GET", 200),
        ("/health/ready", "GET", 200),
        ("/health/live", "POST", 403),
        ("/api/identity/session", "GET", 403),
    ] {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri(path)
                    .method(method)
                    .header("host", "127.0.0.1:8765")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), expected);
        if expected == 200 {
            assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        }
    }
}

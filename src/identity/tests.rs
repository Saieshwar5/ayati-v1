use super::*;
use axum::http::Request;
use tower::ServiceExt;

/// Requests in these tests are rejected or answered before reaching PostgreSQL.
fn app() -> Shared {
    Arc::new(App {
        library: Default::default(),
        provider: Provider::test_provider(),
        store: postgres::PgStore::disconnected(),
        config: std::sync::Arc::new(crate::config::Controller::for_tests(
            "http://127.0.0.1:8765",
        )),
        simulation: None,
        runtimes: workspaces::Runtimes::new(
            std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()),
        ),
    })
}

#[tokio::test]
async fn guards_reject_cross_origin_posts_legacy_routes_and_invalid_callback() {
    let app = app();
    for (path, method, host, expected) in [
        (
            "/api/identity/logout",
            "POST",
            "127.0.0.1:8765",
            StatusCode::FORBIDDEN,
        ),
        (
            "/api/identity/session",
            "GET",
            "evil.example",
            StatusCode::FORBIDDEN,
        ),
        ("/api/state", "GET", "127.0.0.1:8765", StatusCode::NOT_FOUND),
        (
            "/api/identity/simulation",
            "GET",
            "127.0.0.1:8765",
            StatusCode::NOT_FOUND,
        ),
    ] {
        let response = router(app.clone())
            .oneshot(
                Request::builder()
                    .uri(path)
                    .method(method)
                    .header("host", host)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
    }
    let response = router(app.clone())
        .oneshot(
            Request::builder()
                .uri("/auth/callback?code=fake&state=fake")
                .header("host", "127.0.0.1:8765")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert!(
        response.headers()["location"]
            .to_str()
            .unwrap()
            .contains("login_error")
    );
}

#[tokio::test]
async fn postgres_errors_do_not_expose_database_values() {
    let error = Error::from(sea_orm::DbErr::Custom("private row value".into()));
    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let bytes = axum::body::to_bytes(response.into_body(), 10000)
        .await
        .unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("private row value"));
}

#[tokio::test]
async fn only_versioned_static_code_can_be_cached() {
    use axum::http::Method;
    for path in [
        "/assets/index-Abcd1234.js",
        "/assets/home-aBc_12-4.css",
        "/assets/pdf.worker.min-CjEcRF4W.mjs",
    ] {
        assert!(cacheable_asset(&Method::GET, path));
        assert!(cacheable_asset(&Method::HEAD, path));
        assert!(!cacheable_asset(&Method::POST, path));
    }
    for path in [
        "/index.html",
        "/assets/index.html",
        "/assets/app.js",
        "/assets/pdf.worker.mjs",
        "/assets/file-Abcd1234.json",
        "/assets/nested/file-Abcd1234.js",
        "/api/identity/session",
        "/w/tenant/api/apps/file-Abcd1234.js",
    ] {
        assert!(!cacheable_asset(&Method::GET, path));
    }
    let app = app();
    let routes = Router::new()
        .route("/assets/entry-Abcd1234.js", get(|| async { "export {}" }))
        .route("/index.html", get(|| async { "<html></html>" }))
        .route(
            "/api/identity/session",
            get(|| async { "private identity" }),
        )
        .layer(middleware::from_fn_with_state(app, guard));
    for (path, expected) in [
        (
            "/assets/entry-Abcd1234.js",
            "private, max-age=31536000, immutable",
        ),
        ("/assets/missing-Abcd1234.js", "no-store"),
        ("/api/identity/session", "no-store"),
        ("/index.html", "no-store"),
    ] {
        let response = routes
            .clone()
            .oneshot(
                Request::builder()
                    .uri(path)
                    .header("host", "127.0.0.1:8765")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.headers()[header::CACHE_CONTROL], expected);
        assert!(response.headers().contains_key("content-security-policy"));
    }
}

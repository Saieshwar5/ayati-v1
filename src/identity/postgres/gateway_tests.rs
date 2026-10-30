//! HTTP sign-in, hosting and expiry rules against real PostgreSQL.
use super::{tests::*, *};
use crate::identity::{
    App, FLOW, Provider, SESSION, digest, router, set_cookie, workspaces::Runtimes,
};
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use std::{collections::HashMap, sync::Arc};
use tower::ServiceExt;

async fn app(origin: &str) -> Arc<App> {
    Arc::new(App {
        library: Default::default(),
        provider: Provider::test_provider(),
        store: fixture().await,
        config: std::sync::Arc::new(crate::config::Controller::for_tests(origin)),
        runtimes: Runtimes::new(std::env::temp_dir().join(uuid::Uuid::new_v4().to_string())),
        simulation: None,
    })
}

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn hosted_login_uses_public_callback_secure_cookies_and_strict_request_authority() {
    let app = app("https://workspace.example.test").await;
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
#[ignore = "requires local PostgreSQL"]
async fn health_endpoints_allow_only_read_only_unscoped_probes() {
    let router = router(app("https://workspace.example.test").await);
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

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn login_is_pkce_bound_and_callback_state_is_single_use() {
    let app = app("http://127.0.0.1:8765").await;
    let response = router(app.clone())
        .oneshot(
            Request::builder()
                .uri("/auth/login")
                .header("host", "127.0.0.1:8765")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let cookie = response.headers()[header::SET_COOKIE].to_str().unwrap();
    assert!(cookie.contains("HttpOnly") && cookie.contains("SameSite=Lax"));
    let pair = cookie.split(';').next().unwrap();
    let token = pair.split_once('=').unwrap().1;
    let url = reqwest::Url::parse(response.headers()[header::LOCATION].to_str().unwrap()).unwrap();
    let params = url.query_pairs().collect::<HashMap<_, _>>();
    assert_eq!(params["code_challenge_method"], "S256");
    // Read the saved flow, then put it back for the callback below.
    let pending = app
        .store
        .take_login_flow(&digest(token))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(params["code_challenge"], digest(&pending.verifier));
    assert_eq!(params["state"], pending.state);
    app.store
        .save_login_flow(&digest(token), &pending)
        .await
        .unwrap();
    let response = router(app.clone())
        .oneshot(
            Request::builder()
                .uri("/auth/callback?code=unused&state=wrong")
                .header("host", "127.0.0.1:8765")
                .header("cookie", pair)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        response.headers()[header::LOCATION]
            .to_str()
            .unwrap()
            .contains("login_error")
    );
    // A wrong state still consumes the single-use flow.
    assert!(
        app.store
            .take_login_flow(&digest(token))
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn expired_or_forged_sessions_and_invitations_grant_nothing() {
    let db = fixture().await;
    let owner = user(&db, "owner").await;
    let employee = user(&db, "employee").await;
    assert!(db.session("forged").await.unwrap().is_none());
    execute(&db.db, "UPDATE relay_sessions SET expires=0", vec![])
        .await
        .unwrap();
    assert!(db.session("owner").await.unwrap().is_none());
    let w = workspace(&db, &owner).await;
    let invitation = db
        .reserve_invite(&owner, &w, &employee.email, "Member")
        .await
        .unwrap();
    db.delivered(&invitation, Some("expired")).await.unwrap();
    execute(
        &db.db,
        "UPDATE relay_invitations SET expires=0 WHERE id=$1",
        vec![invitation.clone().into()],
    )
    .await
    .unwrap();
    assert!(db.accept(&employee, &invitation).await.is_err());
    let other = user(&db, "other").await;
    let cancelled = db
        .reserve_invite(&owner, &w, &other.email, "Member")
        .await
        .unwrap();
    db.delivered(&cancelled, Some("cancelled")).await.unwrap();
    db.cancel(&owner, &w, &cancelled).await.unwrap();
    assert!(db.accept(&other, &cancelled).await.is_err());
    assert!(db.role(&w, &employee.id).await.is_err());
    assert!(db.role(&w, &other.id).await.is_err());
}

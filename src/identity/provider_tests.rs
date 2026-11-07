use super::*;
use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

struct Mock {
    barrier: tokio::sync::Barrier,
    active: AtomicBool,
    verified: AtomicBool,
    available: AtomicBool,
    calls: AtomicUsize,
}
async fn mock_provider() -> (Provider, Arc<Mock>, tokio::task::JoinHandle<()>) {
    let mock = Arc::new(Mock {
        // Neither response can complete until both independent checks have arrived.
        barrier: tokio::sync::Barrier::new(2),
        active: AtomicBool::new(true),
        verified: AtomicBool::new(true),
        available: AtomicBool::new(true),
        calls: AtomicUsize::new(0),
    });
    let router = Router::new()
        .route("/user_management/users/user_test/sessions", get(sessions))
        .route("/user_management/users/user_test", get(profile))
        .with_state(mock.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let mut provider = Provider::test_provider();
    provider.client = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    provider.base_url = format!("http://{address}");
    (provider, mock, task)
}

async fn sessions(State(mock): State<Arc<Mock>>) -> Result<Json<Value>, StatusCode> {
    mock.calls.fetch_add(1, Ordering::SeqCst);
    mock.barrier.wait().await;
    if !mock.available.load(Ordering::SeqCst) {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }
    let status = if mock.active.load(Ordering::SeqCst) {
        "active"
    } else {
        "revoked"
    };
    Ok(Json(
        json!({"data":[{"id":"session_test", "status":status}]}),
    ))
}

async fn profile(State(mock): State<Arc<Mock>>) -> Json<Value> {
    mock.calls.fetch_add(1, Ordering::SeqCst);
    mock.barrier.wait().await;
    Json(
        json!({"email":"OWNER@example.com", "email_verified":mock.verified.load(Ordering::SeqCst)}),
    )
}

#[tokio::test]
async fn independent_live_checks_overlap_and_revocation_is_not_cached() {
    let (provider, mock, server) = mock_provider().await;
    assert!(
        provider
            .identity_valid("user_test", "session_test", "owner@example.com")
            .await
            .unwrap()
    );
    mock.active.store(false, Ordering::SeqCst);
    assert!(
        !provider
            .identity_valid("user_test", "session_test", "owner@example.com")
            .await
            .unwrap()
    );
    assert_eq!(mock.calls.load(Ordering::SeqCst), 4);
    server.abort();
}

#[tokio::test]
async fn parallel_checks_reject_unverified_changed_or_missing_identity() {
    let (provider, mock, server) = mock_provider().await;
    mock.verified.store(false, Ordering::SeqCst);
    assert!(
        !provider
            .identity_valid("user_test", "session_test", "owner@example.com")
            .await
            .unwrap()
    );
    mock.verified.store(true, Ordering::SeqCst);
    assert!(
        !provider
            .identity_valid("user_test", "session_test", "other@example.com")
            .await
            .unwrap()
    );
    assert!(
        !provider
            .identity_valid("user_test", "session_missing", "owner@example.com")
            .await
            .unwrap()
    );
    server.abort();
}

#[tokio::test]
async fn provider_failure_remains_closed_after_a_successful_check() {
    let (provider, mock, server) = mock_provider().await;
    assert!(
        provider
            .identity_valid("user_test", "session_test", "owner@example.com")
            .await
            .unwrap()
    );
    mock.available.store(false, Ordering::SeqCst);
    assert!(
        provider
            .identity_valid("user_test", "session_test", "owner@example.com")
            .await
            .is_err()
    );
    server.abort();
}

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn profile_updates_require_current_session_and_cannot_target_another_user() {
    use crate::identity::{self, postgres::tests::fixture};
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;
    let (provider, mock, server) = mock_provider().await;
    let store = fixture().await;
    let owner = store
        .login(
            "user_test",
            "owner@example.com",
            &digest("token"),
            "session_test",
        )
        .await
        .unwrap();
    let other = store
        .login("user_other", "other@example.com", "other", "other_session")
        .await
        .unwrap();
    let app = Arc::new(identity::App {
        library: Default::default(),
        provider,
        store,
        config: std::sync::Arc::new(crate::config::Controller::for_tests(
            "http://127.0.0.1:8765",
        )),
        simulation: None,
        runtimes: identity::workspaces::Runtimes::new(
            std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()),
        ),
    });
    for (body, cookie, origin, expected) in [
        (
            json!({"display_name":"Sai"}),
            "",
            "http://127.0.0.1:8765",
            StatusCode::UNAUTHORIZED,
        ),
        (
            json!({"display_name":"Sai"}),
            "ayati_identity=token",
            "https://evil.example",
            StatusCode::FORBIDDEN,
        ),
        (
            json!({"display_name":"Sai","id":other.id}),
            "ayati_identity=token",
            "http://127.0.0.1:8765",
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            json!({"display_name":"Sai"}),
            "ayati_identity=token",
            "http://127.0.0.1:8765",
            StatusCode::OK,
        ),
    ] {
        let response = identity::router(app.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/identity/profile")
                    .header("host", "127.0.0.1:8765")
                    .header("origin", origin)
                    .header("cookie", cookie)
                    .header("x-ayati-request", "identity")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
    }
    assert_eq!(
        app.store
            .session(&digest("token"))
            .await
            .unwrap()
            .unwrap()
            .0
            .display_name
            .as_deref(),
        Some("Sai")
    );
    assert!(
        app.store
            .session("other")
            .await
            .unwrap()
            .unwrap()
            .0
            .display_name
            .is_none()
    );
    mock.active.store(false, Ordering::SeqCst);
    let response = identity::router(app.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/identity/profile")
                .header("host", "127.0.0.1:8765")
                .header("origin", "http://127.0.0.1:8765")
                .header("cookie", "ayati_identity=token")
                .header("x-ayati-request", "identity")
                .header("content-type", "application/json")
                .body(Body::from(json!({"display_name":"Rejected"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        app.store
            .session(&digest("token"))
            .await
            .unwrap()
            .unwrap()
            .0
            .id,
        owner.id
    );
    assert_eq!(
        app.store
            .session(&digest("token"))
            .await
            .unwrap()
            .unwrap()
            .0
            .display_name
            .as_deref(),
        Some("Sai")
    );
    server.abort();
}

#[tokio::test]
async fn reads_trust_a_live_session_for_a_minute_and_changes_always_ask() {
    let (provider, mock, server) = mock_provider().await;
    let user = "user_test";
    let now = std::time::Instant::now();
    let check =
        |at| provider.identity_valid_at(user, "session_test", "owner@example.com", true, at);
    let change =
        |at| provider.identity_valid_at(user, "session_test", "owner@example.com", false, at);
    assert!(check(now).await.unwrap());
    assert_eq!(
        mock.calls.load(Ordering::SeqCst),
        2,
        "one session and one profile read"
    );
    // Many requests and live batches within the minute ask WorkOS nothing more.
    for seconds in [1, 10, 59] {
        assert!(check(now + Duration::from_secs(seconds)).await.unwrap());
    }
    assert_eq!(mock.calls.load(Ordering::SeqCst), 2);
    // A change always asks WorkOS, and sees a revocation at once.
    mock.active.store(false, Ordering::SeqCst);
    assert!(!change(now + Duration::from_secs(20)).await.unwrap());
    assert_eq!(mock.calls.load(Ordering::SeqCst), 4);
    // The refusal is not trusted, and it forgot the earlier answer.
    mock.active.store(true, Ordering::SeqCst);
    assert!(check(now + Duration::from_secs(21)).await.unwrap());
    assert_eq!(mock.calls.load(Ordering::SeqCst), 6);
    // After the minute, a revocation in WorkOS is seen by reads too.
    let now = now + Duration::from_secs(21);
    mock.active.store(false, Ordering::SeqCst);
    assert!(!check(now + Duration::from_secs(61)).await.unwrap());
    assert!(!check(now + Duration::from_secs(62)).await.unwrap());
    assert_eq!(
        mock.calls.load(Ordering::SeqCst),
        10,
        "a refusal is asked again every time"
    );
    // Errors are never trusted either.
    mock.active.store(true, Ordering::SeqCst);
    mock.available.store(false, Ordering::SeqCst);
    assert!(check(now + Duration::from_secs(63)).await.is_err());
    server.abort();
}

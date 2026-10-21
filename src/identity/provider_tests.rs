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
async fn profile_updates_require_current_session_and_cannot_target_another_user() {
    use crate::identity::{self, store::Store};
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;
    let (provider, mock, server) = mock_provider().await;
    let store = Store::open(std::path::Path::new(":memory:")).unwrap();
    let owner = store
        .login(
            "user_test",
            "owner@example.com",
            &digest("token"),
            "session_test",
        )
        .unwrap();
    let other = store
        .login("user_other", "other@example.com", "other", "other_session")
        .unwrap();
    let app = Arc::new(identity::App {
        provider,
        store: store.into(),
        origin: "http://127.0.0.1:8765".into(),
        pending: std::sync::Mutex::new(std::collections::HashMap::new()),
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
            .sqlite()
            .session(&digest("token"))
            .unwrap()
            .unwrap()
            .0
            .display_name
            .as_deref(),
        Some("Sai")
    );
    assert!(
        app.store
            .sqlite()
            .session("other")
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
            .sqlite()
            .session(&digest("token"))
            .unwrap()
            .unwrap()
            .0
            .id,
        owner.id
    );
    assert_eq!(
        app.store
            .sqlite()
            .session(&digest("token"))
            .unwrap()
            .unwrap()
            .0
            .display_name
            .as_deref(),
        Some("Sai")
    );
    server.abort();
}

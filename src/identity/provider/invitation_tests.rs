//! Real HTTP identity/invitation boundary, with a loopback-only provider fixture.
use super::*;
use crate::identity::{self, postgres::tests::fixture};
use axum::{
    Json, Router,
    body::{Body, to_bytes},
    extract::{Path, State},
    http::{Request, StatusCode},
    routing::{get, post},
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tower::ServiceExt;

#[derive(Default)]
struct Mail {
    mode: AtomicUsize,
    calls: AtomicUsize,
}
async fn mail(
    State(mail): State<Arc<Mail>>,
    Json(input): Json<Value>,
) -> (StatusCode, Json<Value>) {
    assert_eq!(input["expires_in_days"], 7);
    let call = mail.calls.fetch_add(1, Ordering::SeqCst);
    match mail.mode.load(Ordering::SeqCst) {
        1 => (StatusCode::SERVICE_UNAVAILABLE, Json(json!({}))),
        2 => (StatusCode::OK, Json(json!({"unexpected":"response"}))),
        3 => {
            tokio::time::sleep(Duration::from_millis(250)).await;
            (StatusCode::OK, Json(json!({"id":format!("mail_{call}")})))
        }
        // WorkOS refuses for certain, as for an address that already has an account.
        4 => (
            StatusCode::BAD_REQUEST,
            Json(
                json!({"code":"user_already_exists","message":"guest@example.test already exists"}),
            ),
        ),
        _ => (StatusCode::OK, Json(json!({"id":format!("mail_{call}")}))),
    }
}
async fn profile(Path(id): Path<String>) -> Json<Value> {
    Json(json!({"email":format!("{id}@example.test"),"email_verified":true}))
}
struct Fixture {
    app: Arc<identity::App>,
    workspace: String,
    mail: Arc<Mail>,
    server: tokio::task::JoinHandle<()>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
    }
}
impl Fixture {
    async fn new() -> Self {
        let mail = Arc::new(Mail::default());
        let router = Router::new()
            .route("/user_management/invitations", post(self::mail))
            .route(
                "/user_management/invitations/{id}/revoke",
                post(|| async { StatusCode::SERVICE_UNAVAILABLE }),
            )
            .route("/user_management/users/{id}", get(profile))
            .route(
                "/user_management/users/{id}/sessions",
                get(|| async { Json(json!({"data":[{"id":"session_test","status":"active"}]})) }),
            )
            .with_state(mail.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let mut provider = Provider::test_provider();
        provider.client = Client::builder()
            .no_proxy()
            .timeout(Duration::from_millis(150))
            .build()
            .unwrap();
        provider.base_url = format!("http://{address}");
        let store = fixture().await;
        let mut users = Vec::new();
        for id in ["owner", "guest", "outsider"] {
            users.push(
                store
                    .login(
                        id,
                        &format!("{id}@example.test"),
                        &digest(id),
                        "session_test",
                    )
                    .await
                    .unwrap(),
            );
        }
        let workspace = uuid::Uuid::new_v4().to_string();
        store
            .create(&users[0], "Fixture", &workspace)
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
        Self {
            app,
            workspace,
            mail,
            server,
        }
    }
    async fn call(&self, actor: &str, path: &str, body: Option<Value>) -> (u16, Value) {
        let request = Request::builder()
            .method(if body.is_some() { "POST" } else { "GET" })
            .uri(path)
            .header("host", "127.0.0.1:8765")
            .header("origin", "http://127.0.0.1:8765")
            .header("cookie", format!("ayati_identity={actor}"))
            .header("x-ayati-request", "identity")
            .header("content-type", "application/json")
            .body(Body::from(body.map(|v| v.to_string()).unwrap_or_default()))
            .unwrap();
        let response = identity::router(self.app.clone())
            .oneshot(request)
            .await
            .unwrap();
        let status = response.status().as_u16();
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }
    fn invitations(&self) -> String {
        format!("/api/identity/workspaces/{}/invitations", self.workspace)
    }
}

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn delivered_invitation_normalizes_email_and_requires_exact_recipient() {
    let f = Fixture::new().await;
    let invite = json!({"email":" Guest@EXAMPLE.test ","role":"Member"});
    let path = f.invitations();
    let (a, b) = tokio::join!(
        f.call("owner", &path, Some(invite.clone())),
        f.call("owner", &path, Some(invite))
    );
    assert!(matches!((a.0, b.0), (200, 400) | (400, 200)));
    assert_eq!(f.mail.calls.load(Ordering::SeqCst), 1);
    let id = if a.0 == 200 { &a.1["id"] } else { &b.1["id"] }
        .as_str()
        .unwrap();
    let accept = format!("/api/identity/invitations/{id}/accept");
    assert_eq!(f.call("outsider", &accept, Some(json!({}))).await.0, 400);
    assert_eq!(f.call("guest", &accept, Some(json!({}))).await.0, 200);
    assert_eq!(f.call("guest", &accept, Some(json!({}))).await.0, 400);
    let people = format!("/api/identity/workspaces/{}/people", f.workspace);
    assert_eq!(f.call("guest", &people, None).await.0, 400);
    assert_eq!(
        f.call("owner", &people, None).await.1["members"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        f.call(
            "guest",
            &f.invitations(),
            Some(json!({"email":"new@example.test","role":"Member"}))
        )
        .await
        .0,
        400
    );
}

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn uncertain_delivery_blocks_duplicate_email_until_owner_cancels() {
    for mode in [1, 2, 3] {
        let f = Fixture::new().await;
        f.mail.mode.store(mode, Ordering::SeqCst);
        let invite = json!({"email":"guest@example.test","role":"Member"});
        assert_eq!(
            f.call("owner", &f.invitations(), Some(invite.clone()))
                .await
                .0,
            400
        );
        let people = format!("/api/identity/workspaces/{}/people", f.workspace);
        let data = f.call("owner", &people, None).await.1;
        assert_eq!(data["invitations"][0]["status"], "delivery_unknown");
        let id = data["invitations"][0]["id"].as_str().unwrap();
        assert_eq!(
            f.call(
                "guest",
                &format!("/api/identity/invitations/{id}/accept"),
                Some(json!({}))
            )
            .await
            .0,
            400
        );
        f.mail.mode.store(0, Ordering::SeqCst);
        assert_eq!(
            f.call("owner", &f.invitations(), Some(invite.clone()))
                .await
                .0,
            400
        );
        assert_eq!(
            f.mail.calls.load(Ordering::SeqCst),
            1,
            "An uncertain send must not be silently repeated"
        );
        assert_eq!(
            f.call(
                "owner",
                &format!("{}/{id}/cancel", f.invitations()),
                Some(json!({}))
            )
            .await
            .0,
            200
        );
        assert_eq!(f.call("owner", &f.invitations(), Some(invite)).await.0, 200);
    }
}

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn local_cancellation_survives_failed_provider_revocation() {
    let f = Fixture::new().await;
    let (_, data) = f
        .call(
            "owner",
            &f.invitations(),
            Some(json!({"email":"guest@example.test","role":"Member"})),
        )
        .await;
    let id = data["id"].as_str().unwrap();
    let (status, cancelled) = f
        .call(
            "owner",
            &format!("{}/{id}/cancel", f.invitations()),
            Some(json!({})),
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(cancelled["provider_invitation_revoked"], false);
    assert_eq!(
        f.call(
            "guest",
            &format!("/api/identity/invitations/{id}/accept"),
            Some(json!({}))
        )
        .await
        .0,
        400
    );
}

#[tokio::test]
#[ignore = "requires local PostgreSQL"]
async fn refused_email_leaves_a_link_only_invitation_the_person_can_accept() {
    let f = Fixture::new().await;
    f.mail.mode.store(4, Ordering::SeqCst);
    let (status, sent) = f
        .call(
            "owner",
            &f.invitations(),
            Some(json!({"email":"guest@example.test","role":"Member"})),
        )
        .await;
    assert_eq!(
        status, 200,
        "a definite refusal still makes a usable invitation"
    );
    assert_eq!(sent["email_sent"], false);
    assert!(sent["link"].as_str().unwrap().contains("/invite/"));
    assert_eq!(f.mail.calls.load(Ordering::SeqCst), 1);
    let people = format!("/api/identity/workspaces/{}/people", f.workspace);
    let listed = f.call("owner", &people, None).await.1;
    assert_eq!(listed["invitations"][0]["status"], "pending");
    assert_eq!(listed["invitations"][0]["delivery"], "link");
    // The invited person finds it on their own page, and only they can accept it.
    let session = f.call("guest", "/api/identity/session", None).await.1;
    assert_eq!(session["invitations"].as_array().unwrap().len(), 1);
    let id = sent["id"].as_str().unwrap();
    let accept = format!("/api/identity/invitations/{id}/accept");
    assert_eq!(f.call("outsider", &accept, Some(json!({}))).await.0, 400);
    assert_eq!(f.call("guest", &accept, Some(json!({}))).await.0, 200);
    assert_eq!(f.call("guest", &accept, Some(json!({}))).await.0, 400);
    assert_eq!(
        f.call("owner", &people, None).await.1["members"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

/// Only WorkOS's plain error code survives; bodies can carry emails or tokens.
#[tokio::test]
async fn provider_errors_keep_only_a_plain_error_code() {
    let router = Router::new()
        .route(
            "/plain",
            post(|| async {
                (
                    StatusCode::BAD_REQUEST,
                    Json(
                        json!({"code":"user_already_exists","message":"guest@example.test exists"}),
                    ),
                )
            }),
        )
        .route(
            "/personal",
            post(|| async {
                (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"code":"guest@example.test"})),
                )
            }),
        )
        .route("/down", post(|| async { StatusCode::SERVICE_UNAVAILABLE }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let provider = Provider::fixture(format!("http://{address}"));
    let call = |path: &'static str| provider.request(reqwest::Method::POST, path, Some(json!({})));

    let plain = call("/plain").await.unwrap_err();
    let refusal = ProviderError::refused(&plain).expect("a 4xx is a definite refusal");
    assert_eq!(refusal.code.as_deref(), Some("user_already_exists"));
    assert_eq!(
        plain.to_string(),
        "Authentication service returned HTTP 400 (user_already_exists)"
    );

    let personal = call("/personal").await.unwrap_err();
    assert_eq!(ProviderError::refused(&personal).unwrap().code, None);
    assert!(!personal.to_string().contains('@'));

    let down = call("/down").await.unwrap_err();
    assert!(
        ProviderError::refused(&down).is_none(),
        "a 5xx leaves the outcome unknown"
    );
    server.abort();
}

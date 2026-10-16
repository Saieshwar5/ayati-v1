use super::*;
use axum::{body::to_bytes, http::Request};
use tower::ServiceExt;

fn provider(url: String) -> Arc<Provider> {
    Arc::new(Provider {
        url: reqwest::Url::parse(&url).unwrap(),
        key: "host-provider-key".into(),
        access: Arc::new(RwLock::new(ModelAccess {
            token: "guest-token".into(),
            model: "fixture".into(),
        })),
        failed: Arc::new(AtomicBool::new(false)),
        client: reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap(),
    })
}

fn request(token: &str, model: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(json!({"model":model,"messages":[]}).to_string()))
        .unwrap()
}

#[tokio::test]
async fn rejects_bad_guest_token_and_unselected_model_before_external_request() {
    let p = provider("http://127.0.0.1:9/unreachable".into());
    assert_eq!(
        router(p.clone())
            .oneshot(request("wrong", "fixture"))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert!(!p.failed.load(Ordering::SeqCst));
    assert_eq!(
        router(p.clone())
            .oneshot(request("guest-token", "different-model"))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert!(p.failed.load(Ordering::SeqCst));
}

#[tokio::test]
async fn replaces_guest_token_and_redacts_provider_errors() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = Router::new().route(
        "/completion",
        post(|headers: HeaderMap| async move {
            assert_eq!(
                headers.get("authorization").unwrap(),
                "Bearer host-provider-key"
            );
            (StatusCode::FORBIDDEN, "PRIVATE_PROVIDER_BODY")
        }),
    );
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let p = provider(format!("http://{address}/completion"));
    let response = router(p.clone())
        .oneshot(request("guest-token", "fixture"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    let body =
        String::from_utf8(to_bytes(response.into_body(), 4096).await.unwrap().to_vec()).unwrap();
    assert!(!body.contains("PRIVATE_PROVIDER_BODY"));
    assert!(!body.contains("host-provider-key"));
    assert!(p.failed.load(Ordering::SeqCst));
    task.abort();
}

#[tokio::test]
async fn selecting_a_new_model_revokes_the_old_token_and_rejects_old_model() {
    let p = provider("http://127.0.0.1:9/unreachable".into());
    let gateway = ModelGateway {
        port: 0,
        access: p.access.clone(),
        failed: p.failed.clone(),
        task: tokio::spawn(async {}),
    };
    let token = gateway.select_model("new-model").unwrap();
    assert_ne!(token, "guest-token");
    assert_eq!(gateway.select_model("new-model").unwrap(), token);
    assert_eq!(
        router(p.clone())
            .oneshot(request("guest-token", "fixture"))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        router(p.clone())
            .oneshot(request(&token, "fixture"))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let response = router(p.clone())
        .oneshot(
            Request::builder()
                .uri("/v1/models")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 4096).await.unwrap();
    assert!(String::from_utf8_lossy(&body).contains("new-model"));
}

use super::client::Client;
use axum::{Json, Router, http::HeaderMap, routing::post};
use serde_json::{Value, json};

#[tokio::test]
async fn shared_transport_keeps_concurrent_capabilities_request_scoped() {
    let router = Router::new().route(
        "/check",
        post(|headers: HeaderMap| async move {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            Json(json!({"authorization":headers["authorization"].to_str().unwrap()}))
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async { axum::serve(listener, router).await.unwrap() });
    let supervisor_token = "s".repeat(43);
    let first_token = "a".repeat(43);
    let second_token = "b".repeat(43);
    let supervisor = Client::new(
        &crate::config::WorkerControl::for_tests(&url, None),
        &supervisor_token,
    )
    .unwrap();
    let first = supervisor.with_token(&first_token).unwrap();
    let second = supervisor.with_token(&second_token).unwrap();
    assert!(supervisor.with_token("invalid").is_err());
    for _ in 0..3 {
        let body = json!({});
        let (a, b, s) = tokio::join!(
            first.post::<Value>("/check", &body),
            second.post::<Value>("/check", &body),
            supervisor.post::<Value>("/check", &body)
        );
        assert_eq!(a.unwrap()["authorization"], format!("Bearer {first_token}"));
        assert_eq!(
            b.unwrap()["authorization"],
            format!("Bearer {second_token}")
        );
        assert_eq!(
            s.unwrap()["authorization"],
            format!("Bearer {supervisor_token}")
        );
    }
    server.abort();
    let _ = server.await;
}

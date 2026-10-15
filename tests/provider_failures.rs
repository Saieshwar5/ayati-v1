use axum::{Router, http::StatusCode, routing::post};
use ayati::model::{CompatibleModel, Message, Model};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn provider_error_body_is_not_exposed_in_saved_or_visible_errors() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = Router::new().route(
        "/chat/completions",
        post(|| async {
            (
                StatusCode::UNAUTHORIZED,
                "private-provider-body-do-not-disclose",
            )
        }),
    );
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let model = CompatibleModel::new(
        &format!("http://{address}"),
        Some("fixture-placeholder".into()),
        "fixture".into(),
    )
    .unwrap();
    let (tx, _rx) = mpsc::channel(4);
    let error = model
        .complete(
            &[Message::text("user", "hello")],
            tx,
            CancellationToken::new(),
        )
        .await
        .unwrap_err();
    assert!(error.to_string().contains("HTTP 401"));
    assert!(!format!("{error:#}").contains("do-not-disclose"));
    server.abort();
}

#[tokio::test]
async fn cancelling_a_stalled_model_request_releases_the_loop_immediately() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = Router::new().route(
        "/chat/completions",
        post(|| async {
            tokio::time::sleep(Duration::from_secs(30)).await;
            "never reached"
        }),
    );
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let model = CompatibleModel::new(
        &format!("http://{address}"),
        Some("fixture-placeholder".into()),
        "fixture".into(),
    )
    .unwrap();
    let cancel = CancellationToken::new();
    let signal = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(80)).await;
        signal.cancel();
    });
    let (tx, _rx) = mpsc::channel(4);
    let messages = [Message::text("user", "hello")];
    let result = tokio::time::timeout(
        Duration::from_secs(1),
        model.complete(&messages, tx, cancel),
    )
    .await;
    assert!(result.unwrap().unwrap_err().to_string().contains("Stopped"));
    server.abort();
}

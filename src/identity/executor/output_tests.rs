use super::output::Output;
use crate::{
    identity::worker_control::{
        client::Client,
        model::{AssignedRequest, Scope},
        output::Update,
    },
    runtime::Event,
};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

fn output(url: &str) -> Output {
    let id = || uuid::Uuid::new_v4().to_string();
    Output {
        trace: crate::identity::start_trace::StartTrace::new(),
        client: Client::new(
            &crate::config::WorkerControl::for_tests(url, None),
            &crate::identity::provider::random_token(),
        )
        .unwrap(),
        assignment: AssignedRequest {
            scope: Scope {
                executor: id(),
                incarnation: id(),
                slot: id(),
                slot_generation: 1,
                assignment: id(),
                workspace: id(),
                agent: id(),
                chat: id(),
                thread: None,
                reply: id(),
                generation: 1,
            },
            session: id(),
        },
    }
}
fn event(kind: &str, offset: Option<usize>, text: Option<&str>) -> Event {
    Event {
        kind: kind.into(),
        task_id: "task".into(),
        message_id: Some("reply".into()),
        offset,
        text: text.map(str::to_owned),
    }
}

#[tokio::test]
async fn lost_local_output_or_offset_gaps_cancel_instead_of_silently_dropping_text() {
    for lag in [false, true] {
        let (events, receiver) = broadcast::channel(1);
        events
            .send(event("delta", Some(1), Some("Unconfirmed output")))
            .ok()
            .expect("Output observer unavailable");
        if lag {
            events
                .send(event("delta", Some(2), Some("Overwritten output")))
                .ok()
                .expect("Output observer unavailable");
        }
        let cancel = CancellationToken::new();
        assert!(
            output("http://127.0.0.1:9")
                .forward(
                    receiver,
                    "reply".into(),
                    CancellationToken::new(),
                    cancel.clone()
                )
                .await
                .is_err()
        );
        assert!(cancel.is_cancelled());
    }
}

#[tokio::test]
async fn non_public_runtime_events_are_never_forwarded_as_reply_text() {
    let (events, receiver) = broadcast::channel(4);
    events
        .send(event("reasoning", Some(0), Some("PRIVATE_REASONING")))
        .ok()
        .expect("Output observer unavailable");
    events
        .send(event("state", None, None))
        .ok()
        .expect("Output observer unavailable");
    let done = CancellationToken::new();
    done.cancel();
    let cancel = CancellationToken::new();
    output("http://127.0.0.1:9")
        .forward(receiver, "reply".into(), done, cancel.clone())
        .await
        .unwrap();
    assert!(!cancel.is_cancelled());
}

#[tokio::test]
async fn text_batches_preserve_utf8_boundaries_and_flush_before_completion() {
    use axum::{Json, Router, routing::post};
    use std::sync::Arc;
    let saved = Arc::new(tokio::sync::Mutex::new(Vec::<String>::new()));
    let received = saved.clone();
    let router = Router::new().route(
        "/v1/assignments/{id}/output",
        post(move |Json(update): Json<Update>| {
            let received = received.clone();
            async move {
                update.validate().unwrap();
                received.lock().await.push(update.text);
                Json(serde_json::json!({"sequence":update.sequence}))
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let (events, receiver) = broadcast::channel(4);
    let text = format!("{}🦀尾", "a".repeat(4095));
    events
        .send(event("prompt", None, None))
        .ok()
        .expect("Output observer unavailable");
    events
        .send(event("delta", Some(0), Some(&text)))
        .ok()
        .expect("Output observer unavailable");
    let done = CancellationToken::new();
    done.cancel();
    output(&format!("http://{address}"))
        .forward(receiver, "reply".into(), done, CancellationToken::new())
        .await
        .unwrap();
    let chunks = saved.lock().await;
    assert!(chunks.iter().all(|v| v.len() <= 4096));
    assert_eq!(chunks.concat(), text);
    assert_eq!(chunks.len(), 3);
    server.abort();
}

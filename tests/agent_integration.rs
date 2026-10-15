use axum::{
    Json, Router,
    body::{Body, to_bytes},
    extract::State,
    http::{Request, StatusCode},
    routing::post,
};
use ayati::{
    agent::Agent,
    files::Files,
    model::CompatibleModel,
    server::{WebState, router},
    storage::Store,
};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::Duration,
};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;

#[derive(Clone, Default)]
struct ProviderState {
    requests: Arc<Mutex<Vec<Value>>>,
    replies: Arc<Mutex<VecDeque<String>>>,
}

async fn fake_provider(State(state): State<ProviderState>, Json(request): Json<Value>) -> Body {
    state.requests.lock().unwrap().push(request);
    let reply = state
        .replies
        .lock()
        .unwrap()
        .pop_front()
        .expect("Unexpected model call");
    Body::from(reply)
}

fn event(value: Value) -> String {
    format!("data: {value}\n\n")
}
fn final_reply(text: &str) -> String {
    event(json!({ "choices": [{ "delta": { "content": text } }] }))
        + &event(json!({ "choices": [{ "delta": {}, "finish_reason": "stop" }] }))
        + "data: [DONE]\n\n"
}

fn tool_reply(command: &str, finish: &str) -> String {
    let arguments = json!({ "command": command }).to_string();
    let split = arguments.len() / 2;
    event(json!({ "choices": [{ "delta": { "content": "I’ll revise the file and check it." } }] }))
        + &event(
            json!({ "choices": [{ "delta": { "tool_calls": [{ "index": 0, "id": "call_1", "type": "function", "function": { "name": "shell", "arguments": &arguments[..split] } }] } }] }),
        )
        + &event(
            json!({ "choices": [{ "delta": { "tool_calls": [{ "index": 0, "function": { "arguments": &arguments[split..] } }] } }] }),
        )
        + &event(json!({ "choices": [{ "delta": {}, "finish_reason": finish }] }))
        + "data: [DONE]\n\n"
}

struct Fixture {
    _dir: TempDir,
    agent: Arc<Agent>,
    provider: ProviderState,
    server: tokio::task::JoinHandle<()>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.agent.stop_all();
        self.server.abort();
    }
}

async fn setup(replies: Vec<String>) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("state.sqlite")).unwrap());
    let files = Arc::new(Files::new(dir.path(), store.clone()).unwrap());
    let provider = ProviderState {
        replies: Arc::new(Mutex::new(replies.into())),
        ..Default::default()
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = Router::new()
        .route("/chat/completions", post(fake_provider))
        .with_state(provider.clone());
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let model = Arc::new(
        CompatibleModel::new(
            &format!("http://{addr}"),
            Some("test-only-placeholder".into()),
            "fixture-model".into(),
        )
        .unwrap(),
    );
    let agent = Agent::start(store, files, model, 96000);
    Fixture {
        _dir: dir,
        agent,
        provider,
        server,
    }
}

async fn finished(agent: &Agent, id: &str) -> String {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let task = agent.store.task(id).unwrap().unwrap();
            if !matches!(task.status.as_str(), "queued" | "running") {
                return task.status;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("Agent did not finish")
}

#[tokio::test]
async fn model_shell_loop_creates_download_and_keeps_complete_tool_context() {
    let fixture = setup(vec![
        tool_reply(
            "tr a-z A-Z < /workspace/input.txt > /workspace/revised.txt; cat revised.txt",
            "tool_calls",
        ),
        final_reply("The revised file is ready. I checked its contents."),
        final_reply("Yes, we revised your notes."),
    ])
    .await;
    std::fs::write(fixture.agent.files.workspace.join("input.txt"), "my notes").unwrap();
    let task = fixture
        .agent
        .submit("Please revise /workspace/input.txt")
        .unwrap();
    assert_eq!(finished(&fixture.agent, &task.id).await, "completed");
    let outputs = fixture.agent.store.artifacts().unwrap();
    let file = outputs.iter().find(|a| a.name == "revised.txt").unwrap();
    assert_eq!(
        std::fs::read(fixture.agent.files.archives.join(&file.stored_name)).unwrap(),
        b"MY NOTES"
    );
    let messages = fixture.agent.store.messages().unwrap();
    assert!(
        messages
            .iter()
            .any(|m| m.content.contains("revised file is ready"))
    );
    let followup = fixture.agent.submit("What did we just do?").unwrap();
    assert_eq!(finished(&fixture.agent, &followup.id).await, "completed");
    let requests = fixture.provider.requests.lock().unwrap();
    assert_eq!(requests.len(), 3);
    let exchange = requests[1]["messages"].as_array().unwrap();
    let tool = exchange.iter().find(|m| m["role"] == "tool").unwrap();
    assert_eq!(tool["tool_call_id"], "call_1");
    let result: Value = serde_json::from_str(tool["content"].as_str().unwrap()).unwrap();
    assert_eq!(result["status"], "success");
    assert!(requests[2].to_string().contains("Please revise"));
    assert!(!requests[2].to_string().contains("test-only-placeholder"));
}

#[tokio::test]
async fn truncated_tool_call_never_executes() {
    let fixture = setup(vec![tool_reply("touch should-not-exist", "length")]).await;
    let task = fixture.agent.submit("Create a file").unwrap();
    assert_eq!(finished(&fixture.agent, &task.id).await, "failed");
    assert!(
        !fixture
            .agent
            .files
            .workspace
            .join("should-not-exist")
            .exists()
    );
    assert!(
        fixture
            .agent
            .store
            .task(&task.id)
            .unwrap()
            .unwrap()
            .detail
            .contains("incomplete")
    );
}

#[tokio::test]
async fn unknown_tool_returns_corrective_feedback_without_executing() {
    let invalid = tool_reply("touch should-not-exist", "tool_calls")
        .replace("\"name\":\"shell\"", "\"name\":\"unknown\"");
    let fixture = setup(vec![
        invalid,
        final_reply("I can use the available shell tool instead."),
    ])
    .await;
    let task = fixture.agent.submit("Try a tool").unwrap();
    assert_eq!(finished(&fixture.agent, &task.id).await, "completed");
    assert!(
        !fixture
            .agent
            .files
            .workspace
            .join("should-not-exist")
            .exists()
    );
    let requests = fixture.provider.requests.lock().unwrap();
    let messages = requests[1]["messages"].as_array().unwrap();
    let feedback = messages.last().unwrap()["content"].as_str().unwrap();
    assert!(feedback.contains("Unknown tool"));
    assert_eq!(messages.last().unwrap()["tool_call_id"], "call_1");
}

#[tokio::test]
async fn stopping_queued_work_prevents_a_second_model_request() {
    let fixture = setup(vec![tool_reply("sleep 30", "tool_calls")]).await;
    let first = fixture.agent.submit("Long task").unwrap();
    let second = fixture.agent.submit("Never execute this").unwrap();
    fixture.agent.stop(&second.id).unwrap();
    tokio::time::sleep(Duration::from_millis(150)).await;
    fixture.agent.stop(&first.id).unwrap();
    assert_eq!(finished(&fixture.agent, &first.id).await, "stopped");
    assert_eq!(finished(&fixture.agent, &second.id).await, "stopped");
    assert_eq!(fixture.provider.requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn local_api_rejects_cross_origin_requests_and_serves_safe_downloads() {
    let fixture = setup(vec![]).await;
    let (file, _) = fixture
        .agent
        .files
        .upload("<script>.html", b"<script>alert(1)</script>")
        .await
        .unwrap();
    let app = router(WebState {
        agent: fixture.agent.clone(),
        model_ready: true,
        model_name: "fixture".into(),
        port: 8765,
        shutdown: CancellationToken::new(),
    });
    let blocked = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/messages")
                .method("POST")
                .header("host", "127.0.0.1:8765")
                .header("origin", "https://attacker.invalid")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"content":"hello"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(blocked.status(), StatusCode::FORBIDDEN);
    assert!(fixture.agent.store.messages().unwrap().is_empty());
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/files/{}", file.id))
                .header("host", "127.0.0.1:8765")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()["content-type"],
        "application/octet-stream"
    );
    assert!(
        response.headers()["content-disposition"]
            .to_str()
            .unwrap()
            .starts_with("attachment;")
    );
    assert_eq!(
        to_bytes(response.into_body(), 1024).await.unwrap(),
        "<script>alert(1)</script>"
    );
    let hostile_host = app
        .oneshot(
            Request::builder()
                .uri("/api/state")
                .header("host", "attacker.invalid:8765")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(hostile_host.status(), StatusCode::FORBIDDEN);
}

#[test]
fn history_survives_reopen_and_uncertain_work_is_not_replayed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    let id = {
        let store = Store::open(&path).unwrap();
        let task = store.enqueue("Remember this conversation").unwrap();
        store.set_task(&task.id, "running", "Working").unwrap();
        task.id
    };
    let reopened = Store::open(&path).unwrap();
    assert_eq!(
        reopened.messages().unwrap()[0].content,
        "Remember this conversation"
    );
    assert_eq!(reopened.task(&id).unwrap().unwrap().status, "interrupted");
}
